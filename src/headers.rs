//! HTTP Headers.

use std::iter::{IntoIterator, Iterator};

/// A collection of HTTP Headers.
#[derive(Debug)]
pub struct Headers<'a> {
    headers: &'a mut http::HeaderMap,
}

impl<'a> Headers<'a> {
    /// Create a new instance.
    pub(crate) fn new(headers: &'a mut http::HeaderMap) -> Self {
        Self { headers }
    }

    /// Get a header.
    pub fn get(&self, key: &'static str) -> Option<&'_ str> {
        self.headers.get(key).map(|h| h.to_str().unwrap())
    }

    /// Set a header.
    pub fn insert(&mut self, key: &'static str, value: impl AsRef<str>) -> Option<String> {
        let value = value.as_ref().to_owned();
        let res = self.headers.insert(key, value.parse().unwrap());
        res.as_ref().map(|h| h.to_str().unwrap().to_owned())
    }

    /// Iterate over all headers.
    pub fn iter(&self) -> Iter<'_> {
        Iter(self.headers.iter())
    }

    /// Get the underlying `http::HeaderMap`.
    pub fn header_map(&mut self) -> &mut http::HeaderMap {
        self.headers
    }
}

impl<'a> IntoIterator for Headers<'a> {
    type Item = (&'a str, &'a str);
    type IntoIter = Iter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        Iter(self.headers.iter())
    }
}

/// An iterator over headers in `Headers`.
#[derive(Debug)]
pub struct Iter<'a>(http::header::Iter<'a, http::header::HeaderValue>);

impl<'a> Iterator for Iter<'a> {
    type Item = (&'a str, &'a str);

    fn next(&mut self) -> Option<Self::Item> {
        self.0
            .next()
            .map(|(key, value)| (key.as_str(), value.to_str().unwrap()))
    }
}