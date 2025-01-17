use crate::credentials;
use crate::error::Error;
use crate::misc::UnwrapOrEmpty;
use crate::token::{Token, TOKEN_URL};
use crate::token_source::TokenSource;
use crate::token_source::{default_https_client, InternalToken, ResponseExtension};
use async_trait::async_trait;
use hyper::client::HttpConnector;
use hyper::http::{Method, Request};
use serde::{Deserialize, Serialize};
use crate::credentials::CredentialsFile;

#[derive(Clone, Serialize)]
struct Claims<'a> {
    iss: &'a str,
    sub: Option<&'a str>,
    scope: Option<&'a str>,
    aud: &'a str,
    exp: i64,
    iat: i64,
}

impl Claims<'_> {
    fn token(&self, pk: &jwt::EncodingKey, pk_id: &str) -> Result<String, Error> {
        let mut header = jwt::Header::new(jwt::Algorithm::RS256);
        header.kid = Some(pk_id.to_string());
        let v = jwt::encode(&header, self, pk)?;
        Ok(v)
    }
}

// Does not use any OAuth2 flow but instead creates a JWT and sends that as the access token.
// The audience is typically a URL that specifies the scope of the credentials.
// see golang.org/x/oauth2/gen/jwt.go
pub struct ServiceAccountTokenSource {
    email: String,
    pk: jwt::EncodingKey,
    pk_id: String,
    audience: String,
}

impl ServiceAccountTokenSource {
    pub(crate) fn new(cred: &credentials::CredentialsFile, audience: &str) -> Result<ServiceAccountTokenSource, Error> {
        Ok(ServiceAccountTokenSource {
            email: cred.client_email.unwrap_or_empty(),
            pk: cred.try_to_private_key()?,
            pk_id: cred.private_key_id.unwrap_or_empty(),
            audience: match &cred.audience {
                None => audience.to_string(),
                Some(s) => s.to_string(),
            },
        })
    }
}

#[async_trait]
impl TokenSource for ServiceAccountTokenSource {
    async fn token(&self) -> Result<Token, Error> {
        let iat = chrono::Utc::now();
        let exp = iat + chrono::Duration::hours(1);

        let token = Claims {
            iss: self.email.as_ref(),
            sub: Some(self.email.as_ref()),
            scope: None,
            aud: self.audience.as_ref(),
            exp: exp.timestamp(),
            iat: iat.timestamp(),
        }
        .token(&self.pk, &self.pk_id)?;

        return Ok(Token {
            access_token: token,
            token_type: "Bearer".to_string(),
            expiry: Some(exp),
        });
    }
}

#[derive(Clone, Deserialize)]
struct OAuth2Token {
    pub access_token: String,
    pub token_type: String,
    pub id_token: Option<String>,
    pub expires_in: Option<i64>,
}

//jwt implements the OAuth 2.0 JSON Web Token flow
pub struct OAuth2ServiceAccountTokenSource {
    pub email: String,
    pub delegation_email: Option<String>,
    pub pk: jwt::EncodingKey,
    pub pk_id: String,
    pub scopes: String,
    pub token_url: String,

    pub client: hyper::Client<hyper_tls::HttpsConnector<HttpConnector>>,
}

impl OAuth2ServiceAccountTokenSource {
    pub(crate) fn new(
        cred: &credentials::CredentialsFile,
        scopes: &str,
        delegation_email: Option<&str>
    ) -> Result<OAuth2ServiceAccountTokenSource, Error> {
        Ok(OAuth2ServiceAccountTokenSource {
            email: cred.client_email.unwrap_or_empty(),
            delegation_email: match delegation_email {
                Some(email) => Some(email.to_string()),
                None => None
            },
            pk: cred.try_to_private_key()?,
            pk_id: cred.private_key_id.unwrap_or_empty(),
            scopes: scopes.to_string(),
            token_url: match &cred.token_uri {
                None => TOKEN_URL.to_string(),
                Some(s) => s.to_string(),
            },
            client: default_https_client(),
        })
    }
}

#[async_trait]
impl TokenSource for OAuth2ServiceAccountTokenSource {
    async fn token(&self) -> Result<Token, Error> {
        let iat = chrono::Utc::now();
        let exp = iat + chrono::Duration::hours(1);

        let request_token = Claims {
            iss: self.email.as_ref(),
            sub: self.delegation_email.as_deref(),
            scope: Some(self.scopes.as_ref()),
            aud: self.token_url.as_ref(),
            exp: exp.timestamp(),
            iat: iat.timestamp(),
        }
        .token(&self.pk, &self.pk_id)?;

        let body = hyper::Body::from(format!(
            "grant_type=urn:ietf:params:oauth:grant-type:jwt-bearer&assertion={}",
            request_token.as_str()
        ));

        let request = Request::builder()
            .method(Method::POST)
            .uri(self.token_url.as_str())
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body(body)?;

        let it: InternalToken = self.client.request(request).await?.deserialize().await?;

        return Ok(it.to_token(iat));
    }
}
