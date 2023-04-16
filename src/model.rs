use std::collections::HashMap;
use std::fs::{self, File};
use std::io::BufReader;
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

pub type TermFreq = HashMap<String, usize>;
pub type TermFreqIndex = HashMap<PathBuf, TermFreq>;

#[derive(Debug)]
pub struct Lexer<'a> {
    content: &'a [char],
}

impl<'a> Lexer<'a> {
    pub fn new(content: &'a [char]) -> Self {
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

pub fn tf(term: &str, doc_freq: &TermFreq) -> f32 {
    let term_freq = *doc_freq.get(term).unwrap_or(&0) as f32;
    let tot_terms_count_in_doc = doc_freq.values().fold(0, |acc, &curr| acc + curr) as f32;
    term_freq / tot_terms_count_in_doc
}

pub fn idf(term: &str, tf_index: &TermFreqIndex) -> f32 {
    let corpus_size = tf_index.len() as f32;
    let doc_count = (tf_index.values().filter(|tf| tf.contains_key(term)).count()).max(1) as f32;
    //println!("doc_count:{doc_count} -> corpus {corpus_size}");
    (corpus_size / doc_count).log10()
}

pub fn read_entire_xml_file(file_path: &PathBuf) -> anyhow::Result<String> {
    //println!("Processing file : {file_path:?}");
    let file = BufReader::new(File::open(file_path)?);
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

pub fn search_query<'a>(tf_index: &'a TermFreqIndex, query: String) -> Vec<(&'a Path, f32)> {
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
    result
}
