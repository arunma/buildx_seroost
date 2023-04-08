use std::collections::HashMap;
use std::ffi::OsStr;
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::{env, io};

use anyhow::{anyhow, Context};
use tiny_http::{Header, Method, Request, Response, Server, StatusCode};
use xml;
use xml::reader::XmlEvent;
use xml::EventReader;

use serde_json::Result as JsonResult;

const STOP_WORDS_PUNCTUATION: [&str; 13] = [
    ",", "\n", "(", ")", ".", "a", "an", "the", "and", "in", "on", "of", "to",
];

type TermFreq = HashMap<String, usize>;
type TermFreqIndex = HashMap<PathBuf, TermFreq>;

#[derive(Debug)]
struct Lexer<'a> {
    content: &'a [char],
}

impl<'a> Lexer<'a> {
    fn new(content: &'a [char]) -> Self {
        Self { content }
    }

    fn trim_left(&mut self) {
        while self.content.len() > 0 && self.content[0].is_whitespace() {
            self.content = &self.content[1..];
        }
    }

    fn chop(&mut self, n: usize) -> &'a [char] {
        let token = &self.content[0..n];
        self.content = &self.content[n..];
        token
    }

    fn chop_while<P>(&mut self, mut predicate: P) -> &'a [char]
    where
        P: FnMut(&char) -> bool,
    {
        let mut n = 0;
        while n < self.content.len() && predicate(&self.content[n]) {
            n += 1;
        }
        self.chop(n)
    }

    fn next_token(&mut self) -> Option<&'a [char]> {
        self.trim_left();
        if self.content.len() == 0 {
            return None;
        }

        if self.content[0].is_alphabetic() {
            return Some(self.chop_while(|x| x.is_alphanumeric()));
        } else if self.content[0].is_numeric() {
            return Some(self.chop_while(|x| x.is_numeric()));
        };

        Some(self.chop(1))
    }
}

impl<'a> Iterator for Lexer<'a> {
    type Item = &'a [char];

    fn next(&mut self) -> Option<Self::Item> {
        self.next_token()
    }
}

fn index_document(content: &str) -> HashMap<String, usize> {
    todo!()
}

fn read_entire_xml_file(file_path: &PathBuf) -> anyhow::Result<String> {
    println!("Processing file : {file_path:?}");
    let file = File::open(file_path)?;
    let event_reader = EventReader::new(file);
    let mut content = String::new();

    for event in event_reader.into_iter() {
        let event = event.unwrap();
        if let XmlEvent::Characters(text) = event {
            content.push_str(&text);
            content.push_str(" ");
        }
    }
    Ok(content)
}

fn main() -> ExitCode {
    match entry() {
        Ok(()) => ExitCode::SUCCESS,
        Err(_) => ExitCode::FAILURE,
    }
}

fn usage(program: &str) {
    anyhow!("Usage: {program} [SUBCOMMAND] [OPTIONS]");
    anyhow!("Subcommands: ");
    anyhow!("     index   <folder>");
    anyhow!("     search  <index-file>");
    anyhow!("     serve   <index-file> [address]");
}

fn serve_404(request: Request) -> anyhow::Result<()> {
    request
        .respond(Response::from_string("404").with_status_code(404))
        .map_err(|e| anyhow!("Unable to respond with error 404 :{e}"))
}

fn serve_static_file(request: Request, file_path: &str, content_type: &str) -> anyhow::Result<()> {
    let file = File::open(file_path).map_err(|e| anyhow!("File {file_path} isn't present: {e}"))?;
    let content_type_html = Header::from_bytes("Content-Type", content_type)
        .map_err(|e| anyhow!("Failed while setting header {e:?}"))?;
    let response = Response::from_file(file).with_header(content_type_html);
    request
        .respond(response)
        .map_err(|e| anyhow!("ERROR: Error in responding to request: {e}"))
}

fn serve_request(request: Request) -> anyhow::Result<()> {
    match (request.method(), request.url()) {
        /* (Method::Get, "/api/search") => {
            let mut content = String::new();
            let json = request
                .as_reader()
                .read_to_string(&mut content)
                .context("ERROR: Error while reading json");
            let value = serde_json::from_reader(json);
        } */
        (Method::Get, "/index.js") => serve_static_file(request, "index.js", "text/javascript")?,
        (Method::Get, "/index.html") | (Method::Get, "/") => {
            serve_static_file(request, "index.html", "text/html")?
        }
        _ => serve_404(request)?,
    }

    Ok(())
}

fn entry() -> anyhow::Result<()> {
    let mut args = env::args();
    let program = args.next().expect("Program name shoudl be provided");

    let subcommand = args
        .next()
        .ok_or_else(|| usage(&program))
        .map_err(|_| anyhow!("Unable to process sub command"))?;

    match subcommand.as_str() {
        "index" => {
            let dir_path = args
                .next()
                .ok_or_else(|| usage(&program))
                .map_err(|_| anyhow!("Unable to read dir path from args"))?;
            let mut tf_index = TermFreqIndex::new();
            index_folder(Path::new(&dir_path), &mut tf_index)?;
            save_index_to_file(&tf_index, "index.json")?;
        }
        "search" => {
            let index_path = args
                .next()
                .ok_or_else(|| usage(&program))
                .map_err(|_| anyhow!("Error while converting args to index path"))?;

            let index_file = File::open(&index_path)
                .map_err(|e| anyhow!("Provided index file : {index_path} not found: {e}"))?;

            let tf_index: TermFreqIndex = serde_json::from_reader(index_file)
                .map_err(|e| anyhow!("Unable to load index from file: {index_path}: {e}"))?;
            println!(
                "{index_path} contains {count} files",
                count = tf_index.len()
            );
        }
        "serve" => {
            let address = args.next().unwrap_or("127.0.0.1:6969".into());
            let server = Server::http(&address)
                .map_err(|e| anyhow!("ERROR: Unable to bind to the address :{address}"))?;

            println!("Listening at address: {address}");

            for request in server.incoming_requests() {
                println!(
                    "INFO: Received request! method: {:?}, url: {:?}, headers: {:?}",
                    request.method(),
                    request.url(),
                    request.headers()
                );

                serve_request(request)?;
            }
        }

        _ => usage(&program),
    }

    Ok(())
}

fn index_folder(dir_path: &Path, tf_index: &mut TermFreqIndex) -> anyhow::Result<()> {
    let dir = fs::read_dir(dir_path)
        .map_err(|e| anyhow!("ERROR: Could not open source directory: {e:?}"))?;

    for file_entry in dir {
        let file = file_entry
            .map_err(|e| anyhow!("ERROR: Unable to read next file in {dir_path:?}: {e}"))?;

        let file_path = file.path();

        let file_type = file.file_type().map_err(|e| {
            anyhow!("Unable to determine the file type with path {file_path:?}: {e}")
        })?;

        if file_type.is_dir() {
            index_folder(&file_path, tf_index)?;
            continue;
        }

        if file_path.extension() != Some(OsStr::new("xhtml")) {
            println!("File path extension: {ext:?}", ext = file_path.extension());
            continue;
        }

        let content = match read_entire_xml_file(&file_path) {
            Ok(content) => content.chars().collect::<Vec<_>>(),
            Err(_) => continue,
        };

        let mut tf = TermFreq::new();

        for token in Lexer::new(&content) {
            let term = token
                .iter()
                .map(|x| x.to_ascii_uppercase())
                .collect::<String>();
            if let Some(count) = tf.get_mut(&term) {
                *count += 1
            } else {
                tf.insert(term, 1);
            }
        }

        let mut stats = tf.iter().collect::<Vec<_>>();
        stats.sort_by_key(|(_, f)| *f);
        stats.reverse();

        tf_index.insert(file_path, tf);
        for (path, tf) in tf_index.iter() {
            println!("{path:?} has {count} terms ", count = tf.len());
        }
    }

    Ok(())
}

fn save_index_to_file(tf_index: &TermFreqIndex, index_path: &str) -> anyhow::Result<()> {
    println!("Writing index to file ...{index_path}");

    let index_file = File::create(index_path)
        .map_err(|e| anyhow!("Index file {index_path} could not be created"))?;

    serde_json::to_writer(index_file, &tf_index)
        .context("Threw an error while writing index to file")?;
    println!("Written index to file {index_path}");

    Ok(())
}
