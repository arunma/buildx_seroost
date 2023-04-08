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

// const STOP_WORDS_PUNCTUATION: [&str; 13] = [
//     ",", "\corpus_size", "(", ")", ".", "a", "an", "the", "and", "in", "on", "of", "to",
// ];

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

    fn chop(&mut self, corpus_size: usize) -> &'a [char] {
        let token = &self.content[0..corpus_size];
        self.content = &self.content[corpus_size..];
        token
    }

    fn chop_while<P>(&mut self, mut predicate: P) -> &'a [char]
    where
        P: FnMut(&char) -> bool,
    {
        let mut corpus_size = 0;
        while corpus_size < self.content.len() && predicate(&self.content[corpus_size]) {
            corpus_size += 1;
        }
        self.chop(corpus_size)
    }

    fn next_token(&mut self) -> Option<String> {
        self.trim_left();
        if self.content.len() == 0 {
            return None;
        }

        if self.content[0].is_alphabetic() {
            return Some(
                self.chop_while(|x| x.is_alphanumeric())
                    .iter()
                    .map(|x| x.to_ascii_uppercase())
                    .collect::<String>(),
            );
        } else if self.content[0].is_numeric() {
            return Some(self.chop_while(|x| x.is_numeric()).iter().collect());
        };

        Some(self.chop(1).iter().collect())
    }
}

impl<'a> Iterator for Lexer<'a> {
    type Item = String;

    fn next(&mut self) -> Option<Self::Item> {
        self.next_token()
    }
}

fn tf(term: &str, doc_freq: &TermFreq) -> f32 {
    let term_freq = *doc_freq.get(term).unwrap_or(&0) as f32;
    let tot_terms_count_in_doc = doc_freq.values().fold(0, |acc, &curr| acc + curr) as f32;
    term_freq / tot_terms_count_in_doc
}

fn idf(term: &str, tf_index: &TermFreqIndex) -> f32 {
    let corpus_size = tf_index.len() as f32;
    let doc_count = (tf_index.values().filter(|tf| tf.contains_key(term)).count()).max(1) as f32;
    //println!("doc_count:{doc_count} -> corpus {corpus_size}");
    (corpus_size / doc_count).log10()
}

fn read_entire_xml_file(file_path: &PathBuf) -> anyhow::Result<String> {
    //println!("Processing file : {file_path:?}");
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
        Err(e) => {
            eprintln!("Program failed with error: {e}");
            usage(&"seroost");
            ExitCode::FAILURE
        }
    }
}

#[allow(unused)]
fn usage(program: &str) {
    eprintln!("Usage: {program} [SUBCOMMAND] [OPTIONS]");
    eprintln!("Subcommands: ");
    eprintln!("     index   <folder>");
    eprintln!("     search  <index-file>");
    eprintln!("     serve   <index-file> [address]");
}

fn serve_404(request: Request) -> anyhow::Result<()> {
    request
        .respond(Response::from_string("404").with_status_code(404))
        .map_err(|e| anyhow!("Unable to respond with error 404 :{e}"))
}

fn serve_static_file(request: Request, file_path: &str, content_type: &str) -> anyhow::Result<()> {
    let file = File::open(file_path).map_err(|e| anyhow!("File {file_path} isn't present: {e}"))?;
    let content_type_html =
        Header::from_bytes("Content-Type", content_type).map_err(|e| anyhow!("Failed while setting header {e:?}"))?;
    let response = Response::from_file(file).with_header(content_type_html);
    request
        .respond(response)
        .map_err(|e| anyhow!("ERROR: Error in responding to request: {e}"))
}

fn serve_request(tf_index: &TermFreqIndex, mut request: Request) -> anyhow::Result<()> {
    match (request.method(), request.url()) {
        (Method::Post, "/api/search") => {
            let mut buf = Vec::new();
            let json = request
                .as_reader()
                .read_to_end(&mut buf)
                .context("ERROR: Error while reading json")?;

            let query = String::from_utf8(buf).map_err(|_| anyhow!("Unable to parse content"))?;
            let query = query.chars().collect::<Vec<_>>();

            let mut result: Vec<(&Path, f32)> = Vec::new();
            for (path, doc_freq) in tf_index {
                let mut rank = 0.0;
                for term in Lexer::new(&query) {
                    let tf = tf(&term, doc_freq);
                    let idf = idf(&term, tf_index);
                    rank += tf * idf;
                }

                result.push((path, rank));
            }
            result.sort_by(|(_, rank1), (_, rank2)| rank2.partial_cmp(&rank1).unwrap());

            for (path, rank) in result.iter().take(10) {
                println!("In document: {path:100?} => Rank    : {rank:0.4}");
            }

            request
                .respond(Response::from_string("ok"))
                .map_err(|_| anyhow!("Error while responding to request"))?
            //let value = serde_json::from_reader(json);
        }
        (Method::Get, "/index.js") => serve_static_file(request, "index.js", "text/javascript")?,
        (Method::Get, "/index.html") | (Method::Get, "/") => serve_static_file(request, "index.html", "text/html")?,
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

            let index_file =
                File::open(&index_path).map_err(|e| anyhow!("Provided index file : {index_path} not found: {e}"))?;

            let tf_index: TermFreqIndex = serde_json::from_reader(index_file)
                .map_err(|e| anyhow!("Unable to load index from file: {index_path}: {e}"))?;
            println!("{index_path} contains {count} files", count = tf_index.len());
        }
        "serve" => {
            let index_path = args.next().ok_or_else(|| anyhow!("No index file present"))?;
            let index_file = File::open(&index_path).map_err(|e| anyhow!("Index file now found {e}"))?;
            let tf_index: TermFreqIndex =
                serde_json::from_reader(index_file).map_err(|e| anyhow!("unable to read index file {e}"))?;

            let address = args.next().unwrap_or("127.0.0.1:6969".into());
            let server =
                Server::http(&address).map_err(|e| anyhow!("ERROR: Unable to bind to the address :{address}"))?;

            println!("Listening at address: {address}");

            for request in server.incoming_requests() {
                println!(
                    "INFO: Received request - method : {:?}, url: {:?}",
                    request.method(),
                    request.url(),
                );

                serve_request(&tf_index, request)?;
            }
        }

        _ => usage(&program),
    }

    Ok(())
}

fn index_folder(dir_path: &Path, tf_index: &mut TermFreqIndex) -> anyhow::Result<()> {
    let dir = fs::read_dir(dir_path).map_err(|e| anyhow!("ERROR: Could not open source directory: {e:?}"))?;

    for file_entry in dir {
        let file = file_entry.map_err(|e| anyhow!("ERROR: Unable to read next file in {dir_path:?}: {e}"))?;

        let file_path = file.path();

        let file_type = file
            .file_type()
            .map_err(|e| anyhow!("Unable to determine the file type with path {file_path:?}: {e}"))?;

        if file_type.is_dir() {
            println!("File type is dir ");
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

        for term in Lexer::new(&content) {
            if let Some(count) = tf.get_mut(&term) {
                *count += 1
            } else {
                tf.insert(term, 1);
            }
        }

        let mut stats = tf.iter().collect::<Vec<_>>();
        stats.sort_by_key(|(_, f)| *f);
        stats.reverse();

        println!("{file_path:?} has {count} terms ", count = tf.len());
        tf_index.insert(file_path, tf);
    }

    Ok(())
}

fn save_index_to_file(tf_index: &TermFreqIndex, index_path: &str) -> anyhow::Result<()> {
    println!("Writing index to file ...{index_path}");

    let index_file = File::create(index_path).map_err(|e| anyhow!("Index file {index_path} could not be created"))?;

    serde_json::to_writer(index_file, &tf_index).context("Threw an error while writing index to file")?;
    println!("Written index to file {index_path}");

    Ok(())
}
