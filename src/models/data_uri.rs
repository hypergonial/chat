use std::{fmt, str::FromStr};

use bytes::Bytes;
use data_url::DataUrl;
use mime::{FromStrError, Mime};
use serde::{
    de::{self, Unexpected, Visitor},
    Deserialize, Deserializer,
};

/// A wrapper around `DataUrl` that implements `Deserialize` and a couple of other useful traits.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DataUri {
    inner: Bytes,
    mime: Mime,
}

impl DataUri {
    pub fn new(inner: impl Into<Bytes>, mime: Mime) -> Self {
        Self {
            inner: inner.into(),
            mime,
        }
    }

    pub const fn mime(&self) -> &Mime {
        &self.mime
    }

    fn from_data_url<E: de::Error>(data_url: &DataUrl<'_>) -> Result<Self, E> {
        let mime = Mime::from_str(&format!(
            "{}/{}",
            data_url.mime_type().type_,
            data_url.mime_type().subtype
        ))
        .map_err(|_: FromStrError| de::Error::custom("invalid MIME type"))?;
        let (bytes, _) = data_url.decode_to_vec().map_err(|e| de::Error::custom(e.to_string()))?;
        Ok(Self::new(bytes, mime))
    }
}

impl<'de> Deserialize<'de> for DataUri {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_str(UriVisitor)
    }
}

impl From<DataUri> for Bytes {
    fn from(uri: DataUri) -> Self {
        uri.inner
    }
}

struct UriVisitor;

impl<'de> Visitor<'de> for UriVisitor {
    type Value = DataUri;

    #[inline]
    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("uri")
    }

    #[inline]
    fn visit_some<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self::Value, D::Error> {
        deserializer.deserialize_str(self)
    }

    fn visit_str<E: de::Error>(self, val: &str) -> Result<Self::Value, E> {
        let res = DataUrl::process(val).map_err(|_| de::Error::invalid_value(Unexpected::Str(val), &self))?;
        DataUri::from_data_url(&res)
    }
}

#[cfg(test)]
mod tests {
    use super::DataUri;
    use serde::Deserialize;
    use serde_json::json;

    #[derive(Deserialize)]
    struct Foo {
        uri: DataUri,
    }

    #[test]
    fn test_data_uri_deserialize() {
        let json = json!({
            "uri": "data:text/plain;base64,SGVsbG8sIFdvcmxkIQ=="
        });
        let foo = serde_json::from_value::<Foo>(json).expect("Failed to deserialize Foo");
        assert_eq!(foo.uri, DataUri::new(b"Hello, World!".to_vec(), mime::TEXT_PLAIN));
    }
}
