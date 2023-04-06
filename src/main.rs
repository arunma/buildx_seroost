use std::collections::HashMap;
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::{env, io};

use xml;
use xml::reader::XmlEvent;
use xml::EventReader;

use serde_json::Result;

const STOP_WORDS_PUNCTUATION: [&str; 13] = [
    ",", "\n", "(", ")", ".", "a", "an", "the", "and", "in", "on", "of", "to",
];

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

fn read_entire_xml_file(file_path: &PathBuf) -> io::Result<String> {
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

type TermFreq = HashMap<String, usize>;
type TermFreqIndex = HashMap<PathBuf, TermFreq>;

fn main() -> std::io::Result<()> {
    let index_path = "index.json";
    let index_file = File::open(index_path)?;
    let tf_index: TermFreqIndex = serde_json::from_reader(index_file)?;
    println!(
        "{index_path} contains {count} files",
        count = tf_index.len()
    );
    Ok(())
}

fn main2() -> std::io::Result<()> {
    let dir_path = "docs.gl/gl4";
    let mut tf_index = TermFreqIndex::new();

    for file in fs::read_dir(dir_path)? {
        let file_path = file?.path();
        let content = read_entire_xml_file(&file_path)?
            .chars()
            .collect::<Vec<_>>();

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

    let index_path = "index.json";
    println!("Writing index to file ...{index_path}");

    let index_file = File::create(index_path)?;
    serde_json::to_writer(index_file, &tf_index)
        .expect("Threw an error while writing index to file");
    println!("Written index to file {index_path}");

    Ok(())
}
