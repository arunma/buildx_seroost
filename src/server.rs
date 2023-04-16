use std::collections::HashMap;
use std::ffi::OsStr;
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::{env, io};

use anyhow::{anyhow, bail, Context};
use tiny_http::{Header, Method, Request, Response, Server, StatusCode};
use xml;
use xml::reader::XmlEvent;
use xml::EventReader;

use serde_json::Result as JsonResult;

use crate::model::{idf, search_query, tf, Lexer, TermFreqIndex};

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
            request
                .as_reader()
                .read_to_end(&mut buf)
                .context("ERROR: Error while reading json")?;

            let query = String::from_utf8(buf).map_err(|_| anyhow!("Unable to parse content"))?;
            let result = search_query(&tf_index, query);

            /* for (path, rank) in result.iter().take(10) {
                println!("In document: {path:100?} => Rank    : {rank:0.4}");
            } */

            let json = serde_json::to_string(&result.iter().collect::<Vec<_>>())?;
            println!("{json}");

            request
                .respond(Response::from_string(json))
                .map_err(|_| anyhow!("Error while responding to request"))?
        }
        (Method::Get, "/index.js") => serve_static_file(request, "index.js", "text/javascript")?,
        (Method::Get, "/index.html") | (Method::Get, "/") => serve_static_file(request, "index.html", "text/html")?,
        _ => serve_404(request)?,
    }

    Ok(())
}

pub fn start(address: &str, tf_index: &TermFreqIndex) -> anyhow::Result<()> {
    let server = Server::http(&address).map_err(|e| anyhow!("ERROR: Unable to bind to the address :{address}"))?;

    println!("Listening at address: {address}");

    for request in server.incoming_requests() {
        println!(
            "INFO: Received request - method : {:?}, url: {:?}",
            request.method(),
            request.url(),
        );

        serve_request(&tf_index, request)?;
    }

    eprintln!("ERROR: Server has shut down");
    bail!("Server has shut down !")
}
