//! Integration tests for the `#[derive(Error)]` macro.

use core::error::Error;
use zlim_core::derive::Error;
use zlim_core::error::{IntoZlimResult, Severity, ZlimError, ZlimResult};

// ---------------------------------------------------------------------------
// Struct — Error + Display
// ---------------------------------------------------------------------------

#[derive(Debug, Error)]
#[error("something went wrong")]
struct SimpleError;

#[test]
fn simple_display() {
    assert_eq!(SimpleError.to_string(), "something went wrong");
    fn assert_error<T: Error>(_: &T) {}
    assert_error(&SimpleError);
}

// ---------------------------------------------------------------------------
// Struct — Error + Display + ZlimError
// ---------------------------------------------------------------------------

#[derive(Debug, Error)]
#[error("a warning occurred")]
#[zlim_error(warning)]
struct WarnError;

#[test]
fn zlim_error_conversion() {
    let err = WarnError;
    let zerr: ZlimError = err.into();
    assert_eq!(zerr.severity(), Severity::Warning);
    assert_eq!(zerr.get().to_string(), "a warning occurred");
}

#[test]
fn into_zlim_result_for_struct() {
    let result: ZlimResult<()> = Err(WarnError).into_zlim_result();
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().severity(), Severity::Warning);
}

// ---------------------------------------------------------------------------
// Tuple struct — positional field interpolation
// ---------------------------------------------------------------------------

#[derive(Debug, Error)]
#[error("code {_0}: {_1}")]
#[zlim_error(error)]
struct CodeError(u32, String);

#[test]
fn tuple_field_interpolation() {
    let err = CodeError(404, "Not Found".into());
    assert_eq!(err.to_string(), "code 404: Not Found");

    let zerr: ZlimError = err.into();
    assert_eq!(zerr.severity(), Severity::Error);
}

// ---------------------------------------------------------------------------
// Named struct — named field interpolation
// ---------------------------------------------------------------------------

#[derive(Debug, Error)]
#[error("cannot open {path}: {reason}")]
#[zlim_error(panic)]
struct FileError {
    path: String,
    reason: String,
}

#[test]
fn named_field_interpolation() {
    let err = FileError {
        path: "/etc/config".into(),
        reason: "permission denied".into(),
    };
    assert_eq!(
        err.to_string(),
        "cannot open /etc/config: permission denied"
    );

    let zerr: ZlimError = err.into();
    assert_eq!(zerr.severity(), Severity::Panic);
}

// ---------------------------------------------------------------------------
// Enum — default error + default zlim_error
// ---------------------------------------------------------------------------

#[derive(Debug, Error)]
#[error("a database error occurred")]
#[zlim_error(error)]
enum DbError {
    #[error("connection refused")]
    ConnectionRefused,

    #[error("query timed out after {_0} ms")]
    #[zlim_error(warning)]
    Timeout(u64),

    NotFound,
}

#[test]
fn enum_default_display() {
    assert_eq!(DbError::NotFound.to_string(), "a database error occurred");
}

#[test]
fn enum_override_display() {
    assert_eq!(DbError::ConnectionRefused.to_string(), "connection refused");
    assert_eq!(
        DbError::Timeout(5000).to_string(),
        "query timed out after 5000 ms"
    );
}

#[test]
fn enum_default_severity() {
    let zerr: ZlimError = DbError::NotFound.into();
    assert_eq!(zerr.severity(), Severity::Error);
}

#[test]
fn enum_override_severity() {
    let zerr: ZlimError = DbError::Timeout(100).into();
    assert_eq!(zerr.severity(), Severity::Warning);
}

#[test]
fn enum_into_zlim_result() {
    let result: ZlimResult<()> = Err(DbError::ConnectionRefused).into_zlim_result();
    assert!(result.is_err());
}

// ---------------------------------------------------------------------------
// Enum — per-variant only (no default)
// ---------------------------------------------------------------------------

#[derive(Debug, Error)]
enum ParseError {
    #[error("unexpected character at {_0}")]
    #[zlim_error(warning)]
    UnexpectedChar(usize),

    #[error("unexpected end of input")]
    #[zlim_error(error)]
    UnexpectedEof,
}

#[test]
fn per_variant_no_default() {
    assert_eq!(
        ParseError::UnexpectedChar(42).to_string(),
        "unexpected character at 42"
    );
    assert_eq!(
        ParseError::UnexpectedEof.to_string(),
        "unexpected end of input"
    );

    let zerr: ZlimError = ParseError::UnexpectedChar(42).into();
    assert_eq!(zerr.severity(), Severity::Warning);

    let zerr: ZlimError = ParseError::UnexpectedEof.into();
    assert_eq!(zerr.severity(), Severity::Error);
}

// ---------------------------------------------------------------------------
// Format specifiers
// ---------------------------------------------------------------------------

#[derive(Debug, Error)]
#[error("value {val:?} out of range [{lo}, {hi}]")]
struct OutOfRange {
    val: i32,
    lo: i32,
    hi: i32,
}

#[test]
fn format_specifier() {
    let err = OutOfRange {
        val: 42,
        lo: 0,
        hi: 10,
    };
    assert_eq!(err.to_string(), "value 42 out of range [0, 10]");
}

// ---------------------------------------------------------------------------
// Escaped braces
// ---------------------------------------------------------------------------

#[derive(Debug, Error)]
#[error("set {{ {field} }}")]
struct BraceError {
    field: String,
}

#[test]
fn escaped_braces() {
    let err = BraceError { field: "x".into() };
    assert_eq!(err.to_string(), "set { x }");
}

// ---------------------------------------------------------------------------
// IntoZlimResult blanket check
// ---------------------------------------------------------------------------

fn accepts_into_zlim_result(_: impl IntoZlimResult<()>) {}

#[test]
fn into_zlim_result_is_callable() {
    accepts_into_zlim_result(Err(WarnError));
    accepts_into_zlim_result(Err(DbError::NotFound));
    accepts_into_zlim_result(Err(ParseError::UnexpectedEof));
}

// ---------------------------------------------------------------------------
// Explicit extra format args
// ---------------------------------------------------------------------------

#[derive(Debug, Error)]
#[error("error code 0x{code:08X}")]
struct HintError {
    code: u32,
}

#[test]
fn implicit_format_with_spec() {
    let err = HintError { code: 0xDEAD };
    assert_eq!(err.to_string(), "error code 0x0000DEAD");
}

// ---------------------------------------------------------------------------
// Extra args: named parameter
// ---------------------------------------------------------------------------

#[derive(Debug, Error)]
#[error("limit {limit} exceeded (max {})", i32::MAX)]
struct LimitError {
    limit: i32,
}

#[test]
fn extra_named_args() {
    let err = LimitError { limit: 10 };
    assert_eq!(err.to_string(), "limit 10 exceeded (max 2147483647)");
}

// ---------------------------------------------------------------------------
// Error trait object
// ---------------------------------------------------------------------------

#[test]
fn error_trait_object() {
    let err: &dyn Error = &SimpleError;
    assert_eq!(err.to_string(), "something went wrong");
    assert!(err.source().is_none());
}
