use std::collections::HashMap;
use std::ffi::OsStr;
use std::fs::{self, File};
use std::io::BufWriter;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::{env, io};

use anyhow::{anyhow, Context};
use buildx_seroost::model::{read_entire_xml_file, Lexer, TermFreq, TermFreqIndex};
use buildx_seroost::server;
use tiny_http::{Header, Method, Request, Response, Server, StatusCode};
use xml;
use xml::reader::XmlEvent;
use xml::EventReader;

use serde_json::Result as JsonResult;

// const STOP_WORDS_PUNCTUATION: [&str; 13] = [
//     ",", "\corpus_size", "(", ")", ".", "a", "an", "the", "and", "in", "on", "of", "to",
// ];

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

            server::start(&address, &tf_index)?;
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

    let index_file =
        BufWriter::new(File::create(index_path).map_err(|e| anyhow!("Index file {index_path} could not be created"))?);

    serde_json::to_writer(index_file, &tf_index).context("Threw an error while writing index to file")?;
    println!("Written index to file {index_path}");

    Ok(())
}
