pub extern crate ascii;
extern crate libc;

pub use ascii::AsciiStr;
use ascii::AsciiCast;

type HttpCallback = unsafe extern fn(*mut HttpParser) -> libc::c_int;
type HttpDataCallback = unsafe extern fn(*mut HttpParser, *const u8, libc::size_t) -> libc::c_int;

#[repr(C)]
#[derive(Clone, Copy)]
enum ParserType {
    HttpRequest,
    HttpResponse,
    HttpBoth
}

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
    on_chunk_complete: HttpCallback
}

#[inline]
unsafe fn unwrap_parser<'a, H>(http: *mut HttpParser) -> (&'a mut H, &'a Parser<H>) {
    (&mut (*((*http).data as *mut Parser<H>)).handler, &*((*http).data as *const Parser<H>))
}

macro_rules! notify_fn_wrapper {
    ( $callback:ident ) => ({
        unsafe extern "C" fn $callback<'a, H: ParserHandler<'a>>(http: *mut HttpParser) -> libc::c_int {
            let (handler, parser) = unwrap_parser::<H>(http);
            if handler.$callback(parser) { 0 } else { 1 }
        };

        $callback::<H>
    });
}

macro_rules! data_fn_wrapper {
    ( $callback:ident, $convert:ident) => ({
        unsafe extern "C" fn $callback<'a, H: ParserHandler<'a>>(http: *mut HttpParser, data: *const u8, size: libc::size_t) -> libc::c_int {
            let (handler, parser) = unwrap_parser::<H>(http);
            let data = std::slice::from_raw_parts(data as *const u8, size as usize).$convert();
            if handler.$callback(parser, data) { 0 } else { 1 }
        };

        $callback::<H>
    });
}

impl HttpParserSettings {
    fn new<'a, H: ParserHandler<'a>>() -> HttpParserSettings {
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
    fn on_url(&mut self, _: &Parser<Self>, &'data AsciiStr) -> bool { true }

    /// Called when a response status becomes available.
    ///
    /// It's not called in the request mode.
    fn on_status(&mut self, _: &Parser<Self>, &'data [u8]) -> bool { true }

    /// Called for each HTTP header key part.
    fn on_header_field(&mut self, _: &Parser<Self>, &'data AsciiStr) -> bool { true }

    /// Called for each HTTP header value part.
    fn on_header_value(&mut self, _: &Parser<Self>, &'data [u8]) -> bool { true }

    /// Called with body text as an argument when the new part becomes available.
    fn on_body(&mut self, _: &Parser<Self>, &'data [u8]) -> bool { true }

    /// Notified when all available headers have been processed.
    fn on_headers_complete(&mut self, _: &Parser<Self>) -> bool { true }

    /// Notified when the parser receives first bytes to parse.
    fn on_message_begin(&mut self, _: &Parser<Self>) -> bool { true }

    /// Notified when the parser has finished its job.
    fn on_message_complete(&mut self, _: &Parser<Self>) -> bool { true }

    fn on_chunk_header(&mut self, _: &Parser<Self>) -> bool { true }
    fn on_chunk_complete(&mut self, _: &Parser<Self>) -> bool { true }
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
/// ```ignore
/// struct MyHandler;
/// impl ParserHandler for MyHandler {
///    fn on_header_field(&self, header: &[u8]) -> bool {
///        println!("{}: ", header);
///        true
///    }
///    fn on_header_value(&self, value: &[u8]) -> bool {
///        println!("\t {}", value);
///        true
///    }
/// }
///
/// let http_request = b"GET / HTTP/1.0\r\n\
///                      Content-Length: 0\r\n\r\n";
///
/// Parser::request(&MyHandler).parse(http_request);
/// ```

pub struct Parser<H> {
    /// Note that `handler` is unsafe to access at all without `&mut`.
    handler: H,
    state: HttpParser,
    flags: u32
}

impl<H> Parser<H> {
    /// Creates a new parser instance for an HTTP response.
    ///
    /// Provide it with your `ParserHandler` trait implementation as an argument.
    pub fn response(handler: H) -> Self {
        Parser {
            handler: handler,
            state: HttpParser::new(ParserType::HttpResponse),
            flags: 0
        }
    }

    /// Creates a new parser instance for an HTTP request.
    ///
    /// Provide it with your `ParserHandler` trait implementation as an argument.
    pub fn request(handler: H) -> Self {
        Parser {
            handler: handler,
            state: HttpParser::new(ParserType::HttpRequest),
            flags: 0
        }
    }

    /// Creates a new parser instance to handle both HTTP requests and responses.
    ///
    /// Provide it with your `ParserHandler` trait implementation as an argument.
    pub fn request_and_response(handler: H) -> Self {
        Parser {
            handler: handler,
            state: HttpParser::new(ParserType::HttpBoth),
            flags: 0
        }
    }

    /// Parses the provided `data` and returns a number of bytes read.
    pub fn parse<'data>(&mut self, data: &'data [u8]) -> usize where H: ParserHandler<'data> {
        unsafe {
            self.state.data = self as *mut _ as *mut libc::c_void;

            let size = http_parser_execute(&mut self.state,
                                           &HttpParserSettings::new::<H>(),
                                           data.as_ptr(),
                                           data.len() as u64) as usize;

            self.flags = http_get_struct_flags(&self.state);

            return size;
        }
    }

    /// Returns an HTTP request or response version.
    pub fn http_version(&self) -> (u16, u16) {
        (self.state.http_major, self.state.http_minor)
    }

    /// Returns an HTTP response status code (think *404*).
    pub fn status_code(&self) -> u16 {
        return (self.flags & 0xFFFF) as u16
    }

    /// Returns an HTTP method static string (`GET`, `POST`, and so on).
    pub fn http_method(&self) -> &'static str {
        let method_code = ((self.flags >> 16) & 0xFF) as u8;
        return http_method_name(method_code);
    }

    fn http_errnum(&self) -> u8 {
        return ((self.flags >> 24) & 0x7F) as u8
    }

    /// Checks if the last `parse` call was finished successfully.
    /// Returns `true` if it wasn't.
    pub fn has_error(&self) -> bool {
        self.http_errnum() != 0x00
    }

    /// In case of a parsing error returns its mnemonic name.
    pub fn error(&self) -> &'static str {
        _http_errno_name(self.http_errnum())
    }

    /// In case of a parsing error returns its description.
    pub fn error_description(&self) -> &'static str {
        _http_errno_description(self.http_errnum())
    }

    /// Checks if an upgrade protocol (e.g. WebSocket) was requested.
    pub fn is_upgrade(&self) -> bool {
        return ((self.flags >> 31) & 0x01) == 1;
    }

    /// If should_keep_alive() in the `on_headers_complete` or
    /// `on_message_complete` callback returns false, then this should be
    /// the last message on the connection.
    /// If you are the server, respond with the "Connection: close" header.
    /// If you are the client, close the connection.
    pub fn should_keep_alive(&self) -> bool {
        self.state.should_keep_alive()
    }

    /// Checks if it was the final body chunk.
    pub fn is_final_chunk(&self) -> bool {
        return self.state.http_body_is_final() == 1;
    }

    pub fn get(&mut self) -> &mut H {
        &mut self.handler
    }
}

impl<H> std::fmt::Debug for Parser<H> {
    fn fmt(&self, fmt: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        let (version_major, version_minor) = self.http_version();
        return write!(fmt, "status_code: {}\n\
                            method: {}\n\
                            error: {}, {}\n\
                            upgrade: {}\n\
                            http_version: {}.{}",
                      self.status_code(),
                      self.http_method(),
                      self.error(), self.error_description(),
                      self.is_upgrade(),
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
    use super::{version, ParserHandler, Parser};

    #[test]
    fn test_version() {
        assert_eq!((2, 5, 0), version());
    }

    #[test]
    fn test_request_parser() {
        struct TestRequestParser;

        impl<'a> ParserHandler<'a> for TestRequestParser {
            fn on_url(&mut self, _parser: &Parser<Self>, url: &AsciiStr) -> bool {
                assert_eq!(b"/say_hello", url.as_bytes());
                true
            }

            fn on_header_field(&mut self, _parser: &Parser<Self>, hdr: &AsciiStr) -> bool {
                assert!(hdr.as_bytes() == b"Host" || hdr.as_bytes() == b"Content-Length");
                true
            }

            fn on_header_value(&mut self, _parser: &Parser<Self>, val: &[u8]) -> bool {
                assert!(val == b"localhost.localdomain" || val == b"11");
                true
            }

            fn on_body(&mut self, _parser: &Parser<Self>, body: &[u8]) -> bool {
                assert_eq!(body, b"Hello world");
                true
            }
        }

        let req = b"POST /say_hello HTTP/1.1\r\nContent-Length: 11\r\nHost: localhost.localdomain\r\n\r\nHello world";

        let mut handler = TestRequestParser;

        let mut parser = Parser::request(handler);
        let parsed = parser.parse(req);

        assert!(parsed > 0);
        assert!(!parser.has_error());
        assert_eq!((1, 1), parser.http_version());
        assert_eq!("POST", parser.http_method());
    }

    #[test]
    fn test_response_parser() {
        struct TestResponseParser;

        impl<'a> ParserHandler<'a> for TestResponseParser {
            fn on_status(&mut self, _parser: &Parser<Self>, status: &[u8]) -> bool {
                assert_eq!(b"OK", status);
                true
            }

            fn on_header_field(&mut self, _parser: &Parser<Self>, hdr: &AsciiStr) -> bool {
                assert_eq!(b"Host", hdr.as_bytes());
                true
            }

            fn on_header_value(&mut self, _parser: &Parser<Self>, val: &[u8]) -> bool {
                assert_eq!(b"localhost.localdomain", val);
                true
            }
        }

        let req = b"HTTP/1.1 200 OK\r\nHost: localhost.localdomain\r\n\r\n";

        let mut handler = TestResponseParser;

        let mut parser = Parser::response(handler);
        let parsed = parser.parse(req);

        assert!(parsed > 0);
        assert!(!parser.has_error());
        assert_eq!((1, 1), parser.http_version());
        assert_eq!(200, parser.status_code());
    }

    #[test]
    fn test_ws_upgrade() {
        struct DummyHandler;

        impl<'a> ParserHandler<'a> for DummyHandler {};

        let req = b"GET / HTTP/1.1\r\nConnection: Upgrade\r\nUpgrade: websocket\r\n\r\n";

        let mut handler = DummyHandler;

        let mut parser = Parser::request(handler);
        parser.parse(req);

        assert_eq!(parser.is_upgrade(), true);
    }

    #[test]
    fn test_error_status() {
        struct DummyHandler {
            url_parsed: bool,
        }

        impl<'a> ParserHandler<'a> for DummyHandler {
            fn on_url(&mut self, _: &Parser<Self>, _: &AsciiStr) -> bool {
                self.url_parsed = true;
                false
            }

            fn on_header_field(&mut self, _: &Parser<Self>, _: &AsciiStr) -> bool {
                panic!("This callback shouldn't be executed!");
                true
            }
        }

        let req = b"GET / HTTP/1.1\r\nHeader: hello\r\n\r\n";

        let mut handler = DummyHandler { url_parsed: false };

        let mut parser = Parser::request(handler);
        parser.parse(req);

        assert!(parser.get().url_parsed);
    }

    #[test]
    fn test_streaming() {
        struct DummyHandler;

        impl<'a> ParserHandler<'a> for DummyHandler {};

        let req = b"GET / HTTP/1.1\r\nHeader: hello\r\n\r\n";

        let mut handler = DummyHandler;
        let mut parser = Parser::request(handler);

        parser.parse(&req[0..10]);

        assert_eq!(parser.http_version(), (0, 0));
        assert!(!parser.has_error());

        parser.parse(&req[10..]);

        assert_eq!(parser.http_version(), (1, 1));
    }

    #[test]
    fn test_catch_error() {
        struct DummyHandler;

        impl<'a> ParserHandler<'a> for DummyHandler {};

        let req = b"UNKNOWN_METHOD / HTTP/3.0\r\nAnswer: 42\r\n\r\n";

        let mut handler = DummyHandler;
        let mut parser = Parser::request(handler);

        parser.parse(req);

        assert!(parser.has_error());
        assert_eq!(parser.error(), "HPE_INVALID_METHOD");
    }
}
