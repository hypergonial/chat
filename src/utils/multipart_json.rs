use axum::{
    extract::{
        multipart::{MultipartError, MultipartRejection},
        FromRequest, Multipart, Request,
    },
    response::{IntoResponse, Response},
    Json, RequestExt,
};
use bytes::Bytes;
use http::StatusCode;
use mime::Mime;
use serde::de::DeserializeOwned;

use serde_json::json;
use thiserror::Error;

/// Errors that can occur while trying to extract a `MultipartJson` from a request.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum MultipartJsonError {
    #[error("The request payload is malformed: {0}")]
    MalformedPayload(#[from] MultipartRejection),
    #[error("The 'json' field is missing from the request")]
    MissingJsonField,
    #[error("A field is malformed: {0}")]
    MalformedField(String),
    #[error("Failed to parse JSON: {0}")]
    JsonSerializationFailure(#[from] serde_json::Error),
    #[error("Duplicate field: {0}")]
    DuplicateField(String),
    #[error("The content-type for field '{0}' is not a valid")]
    ContentType(String),
    #[error(transparent)]
    ParseError(#[from] MultipartError),
}

impl IntoResponse for MultipartJsonError {
    fn into_response(self) -> Response {
        let status = match self {
            Self::MalformedPayload(_)
            | Self::MissingJsonField
            | Self::JsonSerializationFailure(_)
            | Self::MalformedField(_)
            | Self::ContentType(_)
            | Self::DuplicateField(_) => StatusCode::BAD_REQUEST,
            Self::ParseError(e) => return e.into_response(),
        };
        let body = json!({
            "error": self.to_string()
        });

        (status, Json(body)).into_response()
    }
}

/// A field extracted from a multipart request.
pub struct Field {
    name: Option<String>,
    file_name: Option<String>,
    content_type: Mime,
    data: Bytes,
}

impl Field {
    /// The name of the field.
    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    /// The name of the file..
    pub fn file_name(&self) -> Option<&str> {
        self.file_name.as_deref()
    }

    /// The content type of the field.
    pub const fn content_type(&self) -> &Mime {
        &self.content_type
    }

    /// The bytes contained in the field.
    pub const fn data(&self) -> &Bytes {
        &self.data
    }
}

/// A type that can be extracted from a request where the request body is a multipart form with a required 'json' field and optional file fields.
pub struct MultipartJson<T: DeserializeOwned>(pub T, pub Vec<Field>);

#[async_trait::async_trait]
impl<T, S> FromRequest<S> for MultipartJson<T>
where
    S: Send + Sync,
    T: DeserializeOwned + Send,
{
    type Rejection = MultipartJsonError;

    async fn from_request(req: Request, _state: &S) -> Result<Self, Self::Rejection> {
        let mut form: Multipart = req.extract().await?;
        let mut json: Option<T> = None;
        let mut fields: Vec<Field> = Vec::new();

        while let Some(part) = form.next_field().await? {
            if part.name() == Some("json") && part.content_type().is_some_and(|ct| ct == "application/json") {
                if json.is_some() {
                    return Err(MultipartJsonError::DuplicateField("json".to_string()));
                }

                let Ok(data) = part.bytes().await else {
                    return Err(MultipartJsonError::MalformedField("json".to_string()));
                };

                json = Some(serde_json::from_slice(&data)?);
            } else {
                let name = part.name().map(String::from);
                let file_name = part.file_name().map(String::from);

                let Ok(content_type) = part
                    .content_type()
                    .unwrap_or("application/octet-stream")
                    .parse::<Mime>()
                else {
                    return Err(MultipartJsonError::ContentType(
                        name.unwrap_or_else(|| "unknown".to_string()),
                    ));
                };

                let Ok(data) = part.bytes().await else {
                    return Err(MultipartJsonError::MalformedField(
                        name.unwrap_or_else(|| "unknown".to_string()),
                    ));
                };

                fields.push(Field {
                    name,
                    file_name,
                    content_type,
                    data,
                });
            }
        }

        Ok(Self(json.ok_or(MultipartJsonError::MissingJsonField)?, fields))
    }
}
