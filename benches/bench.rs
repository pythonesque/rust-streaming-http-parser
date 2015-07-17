#![feature(test)]

extern crate test;
extern crate http_muncher;

use http_muncher::AsciiStr;
use test::Bencher;

use http_muncher::{HttpMethod, Parser, ParserHandler, ParserSettings};

#[bench]
fn bench_request_parser(b: &mut Bencher) {
    struct TestRequestParser;

    struct Foo;

    impl<'a> ParserHandler<'a> for Foo {}

    impl<'a> ParserHandler<'a> for TestRequestParser {
        fn on_url(&mut self, _: &Parser, url: &AsciiStr) -> bool {
            assert_eq!(b"/say_hello", url.as_bytes());
            true
        }

        fn on_header_field(&mut self, _: &Parser, hdr: &AsciiStr) -> bool {
            assert!(hdr.as_bytes() == b"Host" || hdr.as_bytes() == b"Content-Length");
            true
        }

        fn on_header_value(&mut self, _: &Parser, val: &[u8]) -> bool {
            assert!(val == b"localhost.localdomain" || val == b"11");
            true
        }

        fn on_body(&mut self, _: &Parser, body: &[u8]) -> bool {
            assert_eq!(body, b"Hello world");
            true
        }
    }

    let req = b"POST /say_hello HTTP/1.1\r\nContent-Length: 11\r\nHost: localhost.localdomain\r\n\r\nHello world";

    let mut parser = Parser::request();
    let settings = ParserSettings::new();
    let mut handler = TestRequestParser;
    b.iter(|| {
        let parsed = parser.parse(&mut handler, &settings, req);

        assert!(parsed > 0);
        let status = parser.status();
        assert!(status.errno().is_ok());
        assert_eq!((1, 1), parser.http_version());
        assert_eq!(Some(HttpMethod::Post), status.method());
    });
        let parsed = parser.parse(&mut handler, &settings, req);
}
