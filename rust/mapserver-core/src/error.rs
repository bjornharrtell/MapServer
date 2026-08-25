//! Error model ported from `src/maperror.c`.

use std::fmt;

pub const NUM_ERROR_CODES: i32 = 46;

const ERROR_CODES: [&str; NUM_ERROR_CODES as usize] = [
    "",
    "Unable to access file.",
    "Memory allocation error.",
    "Incorrect data type.",
    "Symbol definition error.",
    "Regular expression error.",
    "TrueType Font error.",
    "DBASE file error.",
    "GD library error.",
    "Unknown identifier.",
    "Premature End-of-File.",
    "Projection library error.",
    "General error message.",
    "CGI error.",
    "Web application error.",
    "Image handling error.",
    "Hash table error.",
    "Join error.",
    "Search returned no results.",
    "Shapefile error.",
    "Expression parser error.",
    "SDE error.",
    "OGR error.",
    "Query error.",
    "WMS server error.",
    "WMS connection error.",
    "OracleSpatial error.",
    "WFS server error.",
    "WFS connection error.",
    "WMS Map Context error.",
    "HTTP request error.",
    "Child array error.",
    "WCS server error.",
    "GEOS library error.",
    "Invalid rectangle.",
    "Date/time error.",
    "GML encoding error.",
    "SOS server error.",
    "NULL parent pointer error.",
    "AGG library error.",
    "OWS error.",
    "OpenGL renderer error.",
    "Renderer error.",
    "V8 engine error.",
    "OCG API error.",
    "Flatgeobuf error.",
];

pub fn error_code_string(code: i32) -> &'static str {
    if !(0..NUM_ERROR_CODES).contains(&code) {
        "Invalid error code."
    } else {
        ERROR_CODES[code as usize]
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapServerError {
    pub code: i32,
    pub routine: String,
    pub message: String,
    pub http_status: String,
    pub error_count: u32,
}

impl MapServerError {
    pub fn new(code: i32, routine: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code,
            routine: routine.into(),
            message: message.into(),
            http_status: String::new(),
            error_count: 0,
        }
    }
}

impl fmt::Display for MapServerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}: {} {}",
            self.routine,
            error_code_string(self.code),
            self.message
        )?;
        if self.error_count > 0 {
            write!(f, " (message repeated {} times)", self.error_count)?;
        }
        Ok(())
    }
}

impl std::error::Error for MapServerError {}

pub type Result<T> = std::result::Result<T, MapServerError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_codes_match_maperror_c() {
        assert_eq!(error_code_string(0), "");
        assert_eq!(error_code_string(2), "Memory allocation error.");
        assert_eq!(error_code_string(45), "Flatgeobuf error.");
        assert_eq!(error_code_string(46), "Invalid error code.");
        assert_eq!(error_code_string(-1), "Invalid error code.");
    }

    #[test]
    fn display_matches_mapserver_format() {
        let mut err = MapServerError::new(12, "msGetEncodedString", "broken input");
        assert_eq!(
            err.to_string(),
            "msGetEncodedString: General error message. broken input"
        );

        err.error_count = 3;
        assert_eq!(
            err.to_string(),
            "msGetEncodedString: General error message. broken input (message repeated 3 times)"
        );
    }
}
