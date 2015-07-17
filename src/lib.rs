pub extern crate ascii;
extern crate libc;

pub use ascii::AsciiStr;
use ascii::AsciiCast;
use std::error::Error;
use std::{fmt, mem};
use std::marker::PhantomData;

#[repr(C)]
struct HttpParser {
    // Private Interface
    _internal_state: libc::uint32_t,
    _nread: libc::uint32_t,
    _content_length: libc::uint64_t,

    // Read-Only
    http_major: libc::c_ushort,
    http_minor: libc::c_ushort,
    _extended_status: libc::uint32_t,

    // Public Interface
    data: *mut libc::c_void
}

impl HttpParser {
    fn new(parser_type: ParserType) -> HttpParser {
        let mut p: HttpParser = unsafe { std::mem::uninitialized() };
        unsafe { http_parser_init(&mut p as *mut _, parser_type); }
        return p;
    }

    fn http_body_is_final(&self) -> libc::c_int {
        unsafe { return http_body_is_final(self); }
    }

    fn should_keep_alive(&self) -> bool {
        unsafe { http_should_keep_alive(self) != 0 }
    }
}


type HttpCallback = unsafe extern fn(*mut HttpParser) -> libc::c_int;
type HttpDataCallback = unsafe extern fn(*mut HttpParser, *const u8, libc::size_t) -> libc::c_int;

#[repr(u8)]
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// An HTTP Request method.
pub enum HttpMethod {
    Delete = 0,
    Get = 1,
    Head = 2,
    Post = 3,
    /* pathological */
    Put = 4,
    Connect = 5,
    Options = 6,
    Trace = 7,
    /* webdav */
    Copy = 8,
    Lock = 9,
    MkCol = 10,
    Move = 11,
    Propfind = 12,
    PropPatch = 13,
    Search = 14,
    Unlock = 15,
    /* subversion */
    Report = 16,
    MkActivity = 17,
    Checkout = 18,
    Merge = 19,
    /* upnp */
    MSearch = 20,
    Notify = 21,
    Subscribe = 22,
    Unsubscribe = 23,
    /* RFC-5789 */
    Patch = 24,
    Purge = 25,
    /* CalDAV */
    MkCalendar = 26,
}

/// Must be equal to the largest HttpMethod!
const MAX_HTTP_METHOD: u8 = 26;

impl HttpMethod {
    /// Returns the static string representation for this name (`GET`, `POST`, and so on).
    pub fn name(&self) -> &'static str {
        http_method_name(*self as u8)
    }
}

impl fmt::Display for HttpMethod {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.name())
    }
}

/// Errors that can occur during parsing.
#[repr(u8)]
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HttpErrno {
  /* No error */
  /// success
  Ok,

  /* Callback-related errors */
  /// the on_message_begin callback failed
  CbMessageBegin,
  /// the on_url callback failed
  CbUrl,
  /// the on_header_field callback failed
  CbHeaderField,
  /// the on_header_value callback failed
  CbHeaderValue,
  /// the on_headers_complete callback failed
  CbHeadersComplete,
  /// the on_body callback failed
  CbBody,
  /// the on_message_complete callback failed
  CbMessageComplete,
  /// the on_status callback failed
  CbStatus,
  /// the on_chunk_header callback failed
  CbChunkHeader,
  /// the on_chunk_complete callback failed
  CbChunkComplete,

  /* Parsing-related errors */
  /// stream ended at an unexpected time
  InvalidEofState,
  /// too many header bytes seen; overflow detected
  HeaderOverflow,
  /// data received after completed connection: close message
  ClosedConnection,
  /// invalid HTTP version
  InvalidVersion,
  /// invalid HTTP status code
  InvalidStatus,
  /// invalid HTTP method
  InvalidMethod,
  /// invalid URL
  InvalidUrl,
  /// invalid host,
  InvalidHost,
  /// invalid port
  InvalidPort,
  /// invalid path
  InvalidPath,
  /// invalid query string
  InvalidQueryString,
  /// invalid fragment
  InvalidFragment,
  /// LF character expected
  LfExpected,
  /// invalid character in header
  InvalidHeaderToken,
  /// invalid character in content-length header
  InvalidContentLength,
  /// invalid character in chunk size header
  InvalidChunkSize,
  /// invalid constant string
  InvalidConstant,
  /// encountered unexpected internal state
  InvalidInternalState,
  /// strict mode assertion failed
  Strict,
  /// parser is paused
  Paused,
  /// an unknown error occurred
  Unknown,
}

impl HttpErrno {
    /// In case of a parsing error returns its mnemonic name.
    pub fn name(&self) -> &'static str {
        _http_errno_name(*self as u8)
    }

    /// Checks if the last `parse` call was finished successfully.
    /// Returns true if it did.
    #[inline]
    pub fn is_ok(&self) -> bool {
        *self == HttpErrno::Ok
    }

}

impl Error for HttpErrno {
    fn description(&self) -> &'static str {
        _http_errno_description(*self as u8)
    }
}

impl fmt::Display for HttpErrno {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", _http_errno_description(*self as u8))
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
enum ParserType {
    HttpRequest,
    HttpResponse,
    HttpBoth
}

/// A structure containing extended status information.
pub struct HttpStatus {
    flags: u32,
}

impl HttpStatus {
    /// Returns an HTTP response status code (think *404*).
    #[inline]
    pub fn code(&self) -> u16 {
        return (self.flags & 0xFFFF) as u16
    }

    fn methodnum(&self) -> u8 {
        ((self.flags >> 16) & 0xFF) as u8
    }

    /// Returns `Some(method)` if the parser's current HTTP method is valid,
    /// or `None` if it is invalid.  Note that this value only has meaning if the method was
    /// already parsed without raising InvalidMethod.
    #[inline]
    pub fn method(&self) -> Option<HttpMethod> {
        let method_code = self.methodnum();
        if method_code > MAX_HTTP_METHOD {
            None
        } else {
            Some(unsafe { mem::transmute(method_code) })
        }
    }

    /// Returns an HTTP method static string ("GET", "POST", and so on), or "\<unknown\>" if it is
    /// invalid.  Note that this value only has meaning if the method was already parsed without
    /// raising InvalidMethod.
    pub fn method_name(&self) -> &'static str {
        let method_code = self.methodnum();
        return http_method_name(method_code);
    }

    /// Return the last parsing error, if any.
    #[inline]
    pub fn errno(&self) -> HttpErrno {
        unsafe {
            // Joyent asserts on bounds check internally, but not for the HTTP method number.
            // This suggests that they think the error code should always be in bounds.  So we
            // do the transmute without checking.
            let error_code = ((self.flags >> 24) & 0x7F) as u8;
            mem::transmute(error_code)
        }
    }

    /// Checks if an upgrade protocol (e.g. WebSocket) was requested.
    #[inline]
    pub fn is_upgrade(&self) -> bool {
        return ((self.flags >> 31) & 0x01) == 1;
    }
}

#[repr(C)]
struct HttpParserSettings {
    on_message_begin: HttpCallback,
    on_url: HttpDataCallback,
    on_status: HttpDataCallback,
    on_header_field: HttpDataCallback,
    on_header_value: HttpDataCallback,
    on_headers_complete: HttpCallback,
    on_body: HttpDataCallback,
    on_message_complete: HttpCallback,
    on_chunk_header: HttpCallback,
    on_chunk_complete: HttpCallback,
}

#[inline]
unsafe fn unwrap_parser<'a, H>(http: *mut HttpParser) -> (&'a mut H, &'a Parser) {
    (&mut (*((*http).data as *mut H)), &*((*http).data as *const Parser))
}

macro_rules! notify_fn_wrapper {
    ( $callback:ident ) => ({
        unsafe extern "C" fn $callback<'data, H: ParserHandler<'data>>(http: *mut HttpParser) -> libc::c_int {
            let (handler, parser) = unwrap_parser::<H>(http);
            if handler.$callback(parser) { 0 } else { 1 }
        };

        $callback::<H>
    });
}

macro_rules! data_fn_wrapper {
    ( $callback:ident, $convert:ident) => ({
        unsafe extern "C" fn $callback<'data, H: ParserHandler<'data>>(http: *mut HttpParser, data: *const u8, size: libc::size_t) -> libc::c_int {
            let (handler, parser) = unwrap_parser::<H>(http);
            let data = std::slice::from_raw_parts(data as *const u8, size as usize).$convert();
            if handler.$callback(parser, data) { 0 } else { 1 }
        };

        $callback::<H>
    });
}

impl HttpParserSettings {
    /// TODO: When it stabilizes, make this a `const fn` so it can be put in a static.
    fn new<'data, H: ParserHandler<'data>>() -> HttpParserSettings {
        HttpParserSettings {
           on_url: data_fn_wrapper!(on_url, to_ascii_nocheck),
           on_message_begin: notify_fn_wrapper!(on_message_begin),
           on_status: data_fn_wrapper!(on_status, into),
           on_header_field: data_fn_wrapper!(on_header_field, to_ascii_nocheck),
           on_header_value: data_fn_wrapper!(on_header_value, into),
           on_headers_complete: notify_fn_wrapper!(on_headers_complete),
           on_body: data_fn_wrapper!(on_body, into),
           on_message_complete: notify_fn_wrapper!(on_message_complete),
           on_chunk_header: notify_fn_wrapper!(on_chunk_header),
           on_chunk_complete: notify_fn_wrapper!(on_chunk_complete)
       }
    }
}

pub struct ParserSettings<'data, H> {
    settings: HttpParserSettings,
    /// This gives us covariance, which is necessary to be able to reuse the same ParserSettings
    /// with data of different lifetimes.
    marker: PhantomData<for <'a> fn (&'a mut H, &'data [u8])>,
}

impl<'data, H> ParserSettings<'data, H> {
    pub fn new() -> Self where H: ParserHandler<'data> {
        ParserSettings {
            settings: HttpParserSettings::new::<H>(),
            marker: PhantomData,
        }
    }
}

#[allow(dead_code)]
extern "C" {
    fn http_parser_version() -> u32;
    fn http_parser_init(parser: *mut HttpParser, parser_type: ParserType);
    fn http_parser_settings_init(settings: *mut HttpParserSettings);
    fn http_parser_execute(parser: *mut HttpParser, settings: *const HttpParserSettings, data: *const u8, len: libc::size_t) -> libc::size_t;
    fn http_should_keep_alive(parser: *const HttpParser) -> libc::c_int;
    fn http_method_str(method_code: u8) -> *const libc::c_char;
    fn http_errno_name(http_errno: u8) -> *const libc::c_char;
    fn http_errno_description(http_errno: u8) -> *const libc::c_char;
    fn http_body_is_final(parser: *const HttpParser) -> libc::c_int;

    // Helper function to predictably use aligned bit-field struct
    fn http_get_struct_flags(parser: *const HttpParser) -> u32;
}

// High level Rust interface

/// Used to define a set of callbacks in your code.
/// They would be called by the parser whenever new data is available.
/// You should bear in mind that the data might get in your callbacks in a partial form.
///
/// Return `bool` as a result of each function call - either
/// `true` for the "OK, go on" status, or `false` when you want to stop
/// the parser after the function call has ended.
///
/// All callbacks provide a default no-op implementation (i.e. they just return `None`).
///
pub trait ParserHandler<'data> {
    /// Called when the URL part of a request becomes available.
    /// E.g. for `GET /forty-two HTTP/1.1` it will be called with `"/forty_two"` argument.
    ///
    /// It's not called in the response mode.
    fn on_url(&mut self, _: &Parser, &'data AsciiStr) -> bool { true }

    /// Called when a response status becomes available.
    ///
    /// It's not called in the request mode.
    fn on_status(&mut self, _: &Parser, &'data [u8]) -> bool { true }

    /// Called for each HTTP header key part.
    fn on_header_field(&mut self, _: &Parser, &'data AsciiStr) -> bool { true }

    /// Called for each HTTP header value part.
    fn on_header_value(&mut self, _: &Parser, &'data [u8]) -> bool { true }

    /// Called with body text as an argument when the new part becomes available.
    fn on_body(&mut self, _: &Parser, &'data [u8]) -> bool { true }

    /// Notified when all available headers have been processed.
    fn on_headers_complete(&mut self, _: &Parser) -> bool { true }

    /// Notified when the parser receives first bytes to parse.
    fn on_message_begin(&mut self, _: &Parser) -> bool { true }

    /// Notified when the parser has finished its job.
    fn on_message_complete(&mut self, _: &Parser) -> bool { true }

    fn on_chunk_header(&mut self, _: &Parser) -> bool { true }
    fn on_chunk_complete(&mut self, _: &Parser) -> bool { true }
}

fn http_method_name(method_code: u8) -> &'static str {
    unsafe {
        let method_str = http_method_str(method_code);
        let buf = std::ffi::CStr::from_ptr(method_str);
        return std::str::from_utf8(buf.to_bytes()).unwrap();
    }
}

fn _http_errno_name(errno: u8) -> &'static str {
    unsafe {
        let err_str = http_errno_name(errno);
        let buf = std::ffi::CStr::from_ptr(err_str);
        return std::str::from_utf8(buf.to_bytes()).unwrap();
    }
}

fn _http_errno_description(errno: u8) -> &'static str {
    unsafe {
        let err_str = http_errno_description(errno);
        let buf = std::ffi::CStr::from_ptr(err_str);
        return std::str::from_utf8(buf.to_bytes()).unwrap();
    }
}

/// The main parser interface.
///
/// # Example
/// ```
/// use http_muncher::{AsciiStr, Parser, ParserHandler, ParserSettings};
///
/// struct MyHandler;
///
/// impl<'a> ParserHandler<'a> for MyHandler {
///    fn on_header_field(&mut self, _: &Parser, header: &AsciiStr) -> bool {
///        println!("{}: ", header);
///        true
///    }
///    fn on_header_value(&mut self, _: &Parser, value: &[u8]) -> bool {
///        println!("\t {:?}", value);
///        true
///    }
/// }
///
/// let http_request = b"GET / HTTP/1.0\r\n\
///                      Content-Length: 0\r\n\r\n";
/// let settings = ParserSettings::new();
/// Parser::request().parse(&mut MyHandler, &settings, http_request);
/// ```

pub struct Parser {
    state: HttpParser,
}

impl Parser {
    /// Creates a new parser instance for an HTTP response.
    ///
    /// Provide it with your `ParserHandler` trait implementation as an argument.
    #[inline]
    pub fn response() -> Self {
        Parser {
            state: HttpParser::new(ParserType::HttpResponse),
        }
    }

    /// Creates a new parser instance for an HTTP request.
    ///
    /// Provide it with your `ParserHandler` trait implementation as an argument.
    #[inline]
    pub fn request() -> Self {
        Parser {
            state: HttpParser::new(ParserType::HttpRequest),
        }
    }

    /// Creates a new parser instance to handle both HTTP requests and responses.
    ///
    /// Provide it with your `ParserHandler` trait implementation as an argument.
    #[inline]
    pub fn request_and_response() -> Self {
        Parser {
            state: HttpParser::new(ParserType::HttpBoth),
        }
    }

    /// Parses the provided `data` and returns a number of bytes read.
    pub fn parse<'data, H>(&mut self, handler: &mut H, settings: &ParserSettings<H>,
                           data: &'data [u8]) -> usize
    {
        unsafe {
            self.state.data = handler as *mut _ as *mut libc::c_void;

            let size = http_parser_execute(&mut self.state,
                                           //&HttpParserSettings::new::<H>(),
                                           &settings.settings,
                                           data.as_ptr(),
                                           data.len() as u64) as usize;

            return size;
        }
    }

    /// Returns a structure containing status information.
    #[inline]
    pub fn status(&self) -> HttpStatus {
        unsafe {
            HttpStatus {
                flags: http_get_struct_flags(&self.state)
            }
        }
    }

    /// Returns an HTTP request or response version.
    #[inline]
    pub fn http_version(&self) -> (u16, u16) {
        (self.state.http_major, self.state.http_minor)
    }

    /// If should_keep_alive() in the `on_headers_complete` or
    /// `on_message_complete` callback returns false, then this should be
    /// the last message on the connection.
    /// If you are the server, respond with the "Connection: close" header.
    /// If you are the client, close the connection.
    #[inline]
    pub fn should_keep_alive(&self) -> bool {
        self.state.should_keep_alive()
    }

    /// Checks if it was the final body chunk.
    #[inline]
    pub fn is_final_chunk(&self) -> bool {
        return self.state.http_body_is_final() == 1;
    }
}

impl std::fmt::Debug for Parser {
    fn fmt(&self, fmt: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        let (version_major, version_minor) = self.http_version();
        let status = self.status();
        let errno = status.errno();
        return write!(fmt, "status_code: {}\n\
                            method: {}\n\
                            error: {}, {}\n\
                            upgrade: {}\n\
                            http_version: {}.{}",
                      status.code(),
                      status.method_name(),
                      errno, errno.description(),
                      status.is_upgrade(),
                      version_major, version_minor);
    }
}

/// Returns a version of the underlying `http-parser` library.
pub fn version() -> (u32, u32, u32) {
    let version = unsafe {
        http_parser_version()
    };

    let major = (version >> 16) & 255;
    let minor = (version >> 8) & 255;
    let patch = version & 255;

    (major, minor, patch)
}

#[cfg(test)]
mod tests {
    use ascii::AsciiStr;
    use super::{version, HttpErrno, HttpMethod, Parser, ParserHandler, ParserSettings};

    #[test]
    fn test_version() {
        assert_eq!((2, 5, 0), version());
    }

    #[test]
    fn test_request_parser() {
        struct TestRequestParser;

        impl<'a> ParserHandler<'a> for TestRequestParser {
            fn on_url(&mut self, _parser: &Parser, url: &AsciiStr) -> bool {
                assert_eq!(b"/say_hello", url.as_bytes());
                true
            }

            fn on_header_field(&mut self, _parser: &Parser, hdr: &AsciiStr) -> bool {
                assert!(hdr.as_bytes() == b"Host" || hdr.as_bytes() == b"Content-Length");
                true
            }

            fn on_header_value(&mut self, _parser: &Parser, val: &[u8]) -> bool {
                assert!(val == b"localhost.localdomain" || val == b"11");
                true
            }

            fn on_body(&mut self, _parser: &Parser, body: &[u8]) -> bool {
                assert_eq!(body, b"Hello world");
                true
            }
        }

        let req = b"POST /say_hello HTTP/1.1\r\nContent-Length: 11\r\nHost: localhost.localdomain\r\n\r\nHello world";

        let mut handler = TestRequestParser;
        let settings = ParserSettings::new();

        let mut parser = Parser::request();
        let parsed = parser.parse(&mut handler, &settings, req);

        assert!(parsed > 0);
        let status = parser.status();
        assert!(status.errno().is_ok());
        assert_eq!((1, 1), parser.http_version());
        assert_eq!(Some(HttpMethod::Post), status.method());
    }

    #[test]
    fn test_response_parser() {
        struct TestResponseParser;

        impl<'a> ParserHandler<'a> for TestResponseParser {
            fn on_status(&mut self, _parser: &Parser, status: &[u8]) -> bool {
                assert_eq!(b"OK", status);
                true
            }

            fn on_header_field(&mut self, _parser: &Parser, hdr: &AsciiStr) -> bool {
                assert_eq!(b"Host", hdr.as_bytes());
                true
            }

            fn on_header_value(&mut self, _parser: &Parser, val: &[u8]) -> bool {
                assert_eq!(b"localhost.localdomain", val);
                true
            }
        }

        let req = b"HTTP/1.1 200 OK\r\nHost: localhost.localdomain\r\n\r\n";

        let mut handler = TestResponseParser;

        let mut parser = Parser::response();
        let settings = ParserSettings::new();
        let parsed = parser.parse(&mut handler, &settings, req);

        assert!(parsed > 0);
        let status = parser.status();
        assert!(status.errno().is_ok());
        assert_eq!((1, 1), parser.http_version());
        assert_eq!(200, status.code());
    }

    #[test]
    fn test_ws_upgrade() {
        struct DummyHandler;

        impl<'a> ParserHandler<'a> for DummyHandler {};

        let req = b"GET / HTTP/1.1\r\nConnection: Upgrade\r\nUpgrade: websocket\r\n\r\n";

        let mut handler = DummyHandler;

        let mut parser = Parser::request();
        let settings = ParserSettings::new();
        parser.parse(&mut handler, &settings, req);

        assert_eq!(parser.status().is_upgrade(), true);
    }

    #[test]
    fn test_error_status() {
        struct DummyHandler {
            url_parsed: bool,
        }

        impl<'a> ParserHandler<'a> for DummyHandler {
            fn on_url(&mut self, _: &Parser, _: &AsciiStr) -> bool {
                self.url_parsed = true;
                false
            }

            fn on_header_field(&mut self, _: &Parser, _: &AsciiStr) -> bool {
                panic!("This callback shouldn't be executed!");
                true
            }
        }

        let req = b"GET / HTTP/1.1\r\nHeader: hello\r\n\r\n";

        let mut handler = DummyHandler { url_parsed: false };
        let settings = ParserSettings::new();

        let mut parser = Parser::request();
        parser.parse(&mut handler, &settings, req);

        assert!(handler.url_parsed);
    }

    #[test]
    fn test_streaming() {
        struct DummyHandler;

        impl<'a> ParserHandler<'a> for DummyHandler {};

        let req = b"GET / HTTP/1.1\r\nHeader: hello\r\n\r\n";

        let mut handler = DummyHandler;
        let settings = ParserSettings::new();
        let mut parser = Parser::request();

        parser.parse(&mut handler, &settings, &req[0..10]);

        assert_eq!(parser.http_version(), (0, 0));
        assert!(parser.status().errno().is_ok());

        parser.parse(&mut handler, &settings, &req[10..]);

        assert_eq!(parser.http_version(), (1, 1));
    }

    #[test]
    fn test_catch_error() {
        struct DummyHandler;

        impl<'a> ParserHandler<'a> for DummyHandler {};

        let req = b"UNKNOWN_METHOD / HTTP/3.0\r\nAnswer: 42\r\n\r\n";

        let mut handler = DummyHandler;
        let settings = ParserSettings::new();
        let mut parser = Parser::request();

        parser.parse(&mut handler, &settings, req);

        let errno = parser.status().errno();
        assert!(!errno.is_ok());
        assert_eq!(errno, HttpErrno::InvalidMethod);
    }
}
