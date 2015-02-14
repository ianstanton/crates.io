#![feature(io, path, core)]
#![cfg_attr(test, deny(warnings))]

extern crate conduit;
extern crate "conduit-mime-types" as mime;
extern crate time;

use std::collections::HashMap;
use std::error::Error;
use std::old_io::FileType;
use std::old_io::fs::File;
use std::old_io::util::NullReader;
use conduit::{Request, Response, Handler};

pub struct Static {
    path: Path,
    types: mime::Types
}

impl Static {
    pub fn new(path: Path) -> Static {
        Static {
            path: path,
            types: mime::Types::new()
                .ok().expect("Couldn't load mime-types")
        }
    }
}

impl Handler for Static {
    fn call(&self, request: &mut Request) -> Result<Response, Box<Error+Send>> {
        let request_path = &request.path()[1..];
        let path = self.path.join(request_path);

        if !self.path.is_ancestor_of(&path) {
            return Ok(not_found())
        }

        let mime = self.types.mime_for_path(&path);
        let file = match File::open(&path) {
            Ok(f) => f,
            Err(..) => return Ok(not_found()),
        };
        let stat = try!(file.stat().map_err(|e| Box::new(e) as Box<Error+Send>));
        match stat.kind {
            FileType::Directory => return Ok(not_found()),
            _ => {}
        }
        let ts = time::Timespec {
            sec: (stat.modified as i64) / 1000,
            nsec: (((stat.modified as u32) % 1000) as i32) * 1000000
        };
        let tm = time::at(ts).to_utc();

        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), vec![mime.to_string()]);
        headers.insert("Content-Length".to_string(), vec![stat.size.to_string()]);
        headers.insert("Last-Modified".to_string(),
                       vec![tm.strftime("%a, %d %b %Y %T GMT").unwrap()
                              .to_string()]);

        Ok(Response {
            status: (200, "OK"),
            headers: headers,
            body: Box::new(file),
        })
    }
}

fn not_found() -> Response {
    Response {
        status: (404, "Not Found"),
        headers: HashMap::new(),
        body: Box::new(NullReader),
    }
}

#[cfg(test)]
mod tests {
    extern crate "conduit-test" as test;

    use std::old_io::{fs, File, TempDir, USER_RWX};

    use conduit::{Handler, Method};
    use Static;

    #[test]
    fn test_static() {
        let td = TempDir::new("conduit-static").unwrap();
        let root = td.path();
        let handler = Static::new(root.clone());
        File::create(&root.join("Cargo.toml")).write_all(b"[package]").unwrap();
        let mut req = test::MockRequest::new(Method::Get, "/Cargo.toml");
        let mut res = handler.call(&mut req).ok().expect("No response");
        let body = res.body.read_to_string().ok().expect("No body");
        assert_eq!(body.as_slice(), "[package]");
        assert_eq!(res.headers.get("Content-Type"),
                   Some(&vec!("text/plain".to_string())));
        assert_eq!(res.headers.get("Content-Length"),
                   Some(&vec!["9".to_string()]));
    }

    #[test]
    fn test_mime_types() {
        let td = TempDir::new("conduit-static").unwrap();
        let root = td.path();
        fs::mkdir(&root.join("src"), USER_RWX).unwrap();
        File::create(&root.join("src/fixture.css")).unwrap();

        let handler = Static::new(root.clone());
        let mut req = test::MockRequest::new(Method::Get, "/src/fixture.css");
        let res = handler.call(&mut req).ok().expect("No response");
        assert_eq!(res.headers.get("Content-Type"),
                   Some(&vec!("text/css".to_string())));
        assert_eq!(res.headers.get("Content-Length"),
                   Some(&vec!["0".to_string()]));
    }

    #[test]
    fn test_missing() {
        let td = TempDir::new("conduit-static").unwrap();
        let root = td.path();

        let handler = Static::new(root.clone());
        let mut req = test::MockRequest::new(Method::Get, "/nope");
        let res = handler.call(&mut req).ok().expect("No response");
        assert_eq!(res.status.0, 404);
    }

    #[test]
    fn test_dir() {
        let td = TempDir::new("conduit-static").unwrap();
        let root = td.path();

        fs::mkdir(&root.join("foo"), USER_RWX).unwrap();

        let handler = Static::new(root.clone());
        let mut req = test::MockRequest::new(Method::Get, "/foo");
        let res = handler.call(&mut req).ok().expect("No response");
        assert_eq!(res.status.0, 404);
    }

    #[test]
    fn last_modified() {
        let td = TempDir::new("conduit-static").unwrap();
        let root = td.path();
        File::create(&root.join("test")).unwrap();
        let handler = Static::new(root.clone());
        let mut req = test::MockRequest::new(Method::Get, "/test");
        let res = handler.call(&mut req).ok().expect("No response");
        assert_eq!(res.status.0, 200);
        assert!(res.headers.get("Last-Modified").is_some());
    }
}
