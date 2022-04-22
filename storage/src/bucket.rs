use crate::bucket::SignedURLError::InvalidOption;
use crate::util;
use chrono::{DateTime, Utc};
use regex::Regex;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fmt::format;
use std::iter::Map;
use std::ops::{Add, Index, Sub};
use std::time::Duration;

static space_regex: Regex = Regex::new(r" +").unwrap();
static tab_regex: Regex = Regex::new(r"[\t]+").unwrap();
const signed_url_methods: [&str; 5] = ["DELETE", "GET", "HEAD", "POST", "PUT"];

pub struct BucketHandle {
    name: String,
}
pub enum SigningScheme {
    /// V2 is deprecated. https://cloud.google.com/storage/docs/access-control/signed-urls?types#types
    /// SigningSchemeV2

    /// SigningSchemeV4 uses the V4 scheme to sign URLs.
    SigningSchemeV4,
}

pub trait URLStyle {
    fn host(&self, bucket: &str) -> &str;
    fn path(&self, bucket: &str, object: &str) -> &str;
}

/// SignedURLOptions allows you to restrict the access to the signed URL.
pub struct SignedURLOptions<F, U>
where
    F: Fn(&[u8]) -> Result<Vec<u8>, SignedURLError>,
    U: URLStyle,
{
    /// GoogleAccessID represents the authorizer of the signed URL generation.
    /// It is typically the Google service account client email address from
    /// the Google Developers Console in the form of "xxx@developer.gserviceaccount.com".
    /// Required.
    google_access_id: String,

    /// PrivateKey is the Google service account private key. It is obtainable
    /// from the Google Developers Console.
    /// At https://console.developers.google.com/project/<your-project-id>/apiui/credential,
    /// create a service account client ID or reuse one of your existing service account
    /// credentials. Click on the "Generate new P12 key" to generate and download
    /// a new private key. Once you download the P12 file, use the following command
    /// to convert it into a PEM file.
    ///
    ///    $ openssl pkcs12 -in key.p12 -passin pass:notasecret -out key.pem -nodes
    ///
    /// Provide the contents of the PEM file as a byte slice.
    /// Exactly one of PrivateKey or SignBytes must be non-nil.
    private_key: Vec<u8>,

    /// SignBytes is a function for implementing custom signing. For example, if
    /// your application is running on Google App Engine, you can use
    /// appengine's internal signing function:
    ///     ctx := appengine.NewContext(request)
    ///     acc, _ := appengine.ServiceAccount(ctx)
    ///     url, err := SignedURL("bucket", "object", &SignedURLOptions{
    ///     	GoogleAccessID: acc,
    ///     	SignBytes: func(b []byte) ([]byte, error) {
    ///     		_, signedBytes, err := appengine.SignBytes(ctx, b)
    ///     		return signedBytes, err
    ///     	},
    ///     	// etc.
    ///     })
    ///
    /// Exactly one of PrivateKey or SignBytes must be non-nil.
    sign_bytes: Option<F>,

    /// Method is the HTTP method to be used with the signed URL.
    /// Signed URLs can be used with GET, HEAD, PUT, and DELETE requests.
    /// Required.
    method: String,

    /// Expires is the expiration time on the signed URL. It must be
    /// a datetime in the future. For SigningSchemeV4, the expiration may be no
    /// more than seven days in the future.
    /// Required.
    expires: DateTime<Utc>,

    /// ContentType is the content type header the client must provide
    /// to use the generated signed URL.
    /// Optional.
    content_type: String,

    /// Headers is a list of extension headers the client must provide
    /// in order to use the generated signed URL. Each must be a string of the
    /// form "key:values", with multiple values separated by a semicolon.
    /// Optional.
    headers: Vec<String>,

    /// QueryParameters is a map of additional query parameters. When
    /// SigningScheme is V4, this is used in computing the signature, and the
    /// client must use the same query parameters when using the generated signed
    /// URL.
    /// Optional.
    query_parameters: Map<String, Vec<String>>,

    /// MD5 is the base64 encoded MD5 checksum of the file.
    /// If provided, the client should provide the exact value on the request
    /// header in order to use the signed URL.
    /// Optional.
    md5: String,

    /// Style provides options for the type of URL to use. Options are
    /// PathStyle (default), BucketBoundHostname, and VirtualHostedStyle. See
    /// https://cloud.google.com/storage/docs/request-endpoints for details.
    /// Only supported for V4 signing.
    /// Optional.
    style: U,

    /// Insecure determines whether the signed URL should use HTTPS (default) or
    /// HTTP.
    /// Only supported for V4 signing.
    /// Optional.
    insecure: bool,

    // Scheme determines the version of URL signing to use. Default is
    // SigningSchemeV2.
    scheme: SigningScheme,
}

#[derive(thiserror::Error, Debug)]
pub enum SignedURLError {
    #[error("invalid option {0}")]
    InvalidOption(&'static str),
}

impl BucketHandle {
    pub fn signed_url<F, U>(object: String, opts: &SignedURLOptions<F, U>) -> Result<String, SignedURLError>
    where
        U: URLStyle,
    {
        //TODO
        Ok("".to_string())
    }
}

pub fn signed_url<F, U>(name: String, object: String, opts: &SignedURLOptions<F, U>) -> Result<String, SignedURLError>
where
    U: URLStyle,
{
    let now = Utc::now();
    let _ = validate_options(opts, &now)?;

    //TODO
    Ok("".to_string())
}

struct Url<'a> {
    schema: String,
    host: String,
    path: &'a str,
    raw_path: String,
}

impl Url {
    fn new(path: &str) -> Self {
        let raw_path = path_encode_v4(path);
        Self {
            path,
            raw_path,
            schema: "https".to_string(),
            host: "".to_string(),
        }
    }
}

fn v4_sanitize_headers(hdrs: &[String]) -> Vec<String> {
    let mut sanitized = HashMap::<String, Vec<String>>::new();
    for hdr in hdrs {
        let trimmed = hdr.trim().to_string();
        let split = trimmed.split(":").collect_vec();
        if split.len() < 2 {
            continue;
        }
        let key = split[0].trim().to_lowercase();
        let mut value = space_regex.replace_all(split[1].trim(), " ");
        value = tab_regex.replace_all(value.as_ref(), "\t");
        if !value.is_empty() {
            if sanitized.contains_key(&key) {
                sanitized.get_mut(&key).unwrap().push(value.to_string())
            } else {
                sanitized.insert(key, vec![value.to_string()])
            }
        }
    }
    let mut sanitized_headers = Vec::with_capacity(sanitized.len());
    let mut index = 0;
    for (key, value) in sanitized {
        sanitized_headers[index] = format!("{}:{}", key, value.join(",").to_string());
        index += 1;
    }
    sanitized_headers
}

fn signed_url_v4<F, U>(
    bucket: &str,
    name: &str,
    opts: &SignedURLOptions<F, U>,
    now: DateTime<Utc>,
) -> Result<String, SignedURLError>
where
    U: URLStyle,
{
    let mut buffer: Vec<u8> = vec![];
    buffer.extend_from_slice(format!("{}\n", opts.method).as_bytes());

    let path = opts.style.path(bucket, name);
    let mut url = Url::new(path);
    buffer.extend_from_slice(format!("/{}\n", raw_path).as_bytes());

    let mut header_names = extract_header_names(&opts.headers);
    header_names.push("host");
    if !opts.content_type.is_empty() {
        header_names.push("content-type");
    }
    if !opts.md5.is_empty() {
        header_names.push("content-md5");
    }
    header_names.sort();

    let signed_headers = header_names.join(";");
    let timestamp = now.to_rfc3339();
    let credential_scope = format!("{}/auto/storage/goog4_request", now.format("%Y%m%d"));
    let mut canonical_query_string = util::QueryParam::new();
    canonical_query_string.adds("X-Goog-Algorithm".to_string(), vec!["GOOG4-RSA-SHA256".to_string()]);
    canonical_query_string.adds(
        "X-Goog-Credential".to_string(),
        vec![format!("{}/{}", opts.google_access_id, credential_scope)],
    );
    canonical_query_string.adds("X-Goog-Date".to_string(), vec![timestamp]);
    canonical_query_string.adds(
        "X-Goog-Expires".to_string(),
        vec![opts.expires.sub(now).num_seconds().to_string()],
    );
    canonical_query_string.adds("X-Goog-SignedHeaders".to_string(), vec![signed_headers]);
    for (k, v) in opts.query_parameters {
        canonical_query_string.insert(k, v)
    }
    let escaped_query = canonical_query_string.encode().replace("+", "%20");
    buffer.extend_from_slice(format!("/{}\n", escaped_query).as_bytes());

    url.host = opts.style.host(bucket).to_string();
    if opts.insecure {
        url.schema = "http".to_string()
    }

    let mut header_with_value = vec![format!("host:{}", url.host)];
    header_with_value.extend_from_slice(&opts.headers);
    if !opts.content_type.is_empty() {
        header_with_value.push(format!("content-type:{}", opts.content_type))
    }
    if !opts.md5.is_empty() {
        header_with_value.push(format!("content-md5:{}", opts.md5))
    }
    header_with_value.sort();
    let canonical_headers = header_with_value.join(" ");
    buffer.extend_from_slice(format!("{}\n\n", canonical_headers).as_bytes());
    buffer.extend_from_slice(format!("{}\n", signed_headers).as_bytes());

    /// If the user provides a value for X-Goog-Content-SHA256, we must use
    /// that value in the request string. If not, we use UNSIGNED-PAYLOAD.
    let sha256_header = header_with_value
        .iter()
        .find_or_first(|h| {
            let ret = h.to_lowercase().starts_with("x-goog-content-sha256") && h.contains(":");
            if ret {
                buffer.extend_from_slice(h.splitn(2, ":")[1])
            }
            ret
        })
        .is_some();
    if !sha256_header {
        buffer.extend_from_slice("UNSIGNED-PAYLOAD".as_bytes());
    }
    let hex_digest = Sha256::digest(buffer);
    let mut signed_buffer: Vec<u8> = vec![];
    signed_buffer.extend_from_slice("GOOG4-RSA-SHA256\n".as_bytes());
    signed_buffer.extend_from_slice(format!("{}\n", timestamp).as_bytes());
    signed_buffer.extend_from_slice(format!("{}\n", credential_scope).as_bytes());
    signed_buffer.extend_from_slice(hex_digest.as_slice());

    Ok("TODO".to_string())
}

fn path_encode_v4(path: &str) -> String {
    let segments = path.split("/").collect_vec();
    let mut encoded_segments = Vec::with_capacity(segments.len());
    for (index, segment) in segments.into_iter().enumerate() {
        encoded_segments[index] = url_escape::encode_query(segment).to_string();
    }
    let encoded_str = encoded_segments.join("/");
    return encoded_str.replace("+", "%20");
}

fn extract_header_names(kvs: &[String]) -> Vec<&str> {
    let mut res = vec![];
    for header in kvs {
        let name_value = header.split(":").collect_vec();
        res.push(name_value[0])
    }
    res
}

fn validate_options<F, U>(opts: &SignedURLOptions<F, U>, now: &DateTime<Utc>) -> Result<(), SignedURLError> {
    if opts.google_access_id.is_empty() {
        return Err(InvalidOption("storage: missing required GoogleAccessID"));
    }
    if opts.private_key.is_empty() && opts.sign_bytes.is_none() {
        return Err(InvalidOption("storage: exactly one of PrivateKey or SignedBytes must be set"));
    }
    if !signed_url_methods.contains(&opts.method.to_uppercase().as_str()) {
        return Err(InvalidOption("storage: invalid HTTP method"));
    }
    if opts.expires.is_zero() {
        return Err(InvalidOption("missing required expires option"));
    }
    if !opts.md5.is_empty() {
        match base64::decode(&opts.md5) {
            Ok(v) => {
                if v.len() != 16 {
                    return Err(InvalidOption("storage: invalid MD5 checksum length"));
                }
            }
            Err(_e) => return Err(InvalidOption("storage: invalid MD5 checksum")),
        }
    }
    if opts.scheme == SigningScheme::SigningSchemeV4 {
        let cutoff = now.add(Duration::from_secs(604801));
        if !opts.expires.lt(cutoff) {
            return Err(InvalidOption("storage: expires must be within seven days from now"));
        }
    }
    Ok(())
}
