#![feature(test)]

extern crate test;
extern crate http_muncher;

use http_muncher::AsciiStr;
use test::Bencher;

use http_muncher::{ParserHandler, Parser};

#[bench]
fn bench_request_parser(b: &mut Bencher) {
    struct TestRequestParser;

    impl<'a> ParserHandler<'a> for TestRequestParser {
        fn on_url(&mut self, _: &Parser<Self>, url: &AsciiStr) -> bool {
            assert_eq!(b"/say_hello", url.as_bytes());
            true
        }

        fn on_header_field(&mut self, _: &Parser<Self>, hdr: &AsciiStr) -> bool {
            assert!(hdr.as_bytes() == b"Host" || hdr.as_bytes() == b"Content-Length");
            true
        }

        fn on_header_value(&mut self, _: &Parser<Self>, val: &[u8]) -> bool {
            assert!(val == b"localhost.localdomain" || val == b"11");
            true
        }

        fn on_body(&mut self, _: &Parser<Self>, body: &[u8]) -> bool {
            assert_eq!(body, b"Hello world");
            true
        }
    }

    let req = b"POST /say_hello HTTP/1.1\r\nContent-Length: 11\r\nHost: localhost.localdomain\r\n\r\nHello world";

    b.iter(|| {
        let mut handler = TestRequestParser;

        let mut parser = Parser::request(handler);
        let parsed = parser.parse(req);

        assert!(parsed > 0);
        assert!(!parser.has_error());
        assert_eq!((1, 1), parser.http_version());
        assert_eq!("POST", parser.http_method());
    });
}
