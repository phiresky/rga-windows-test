/**
 * Module for matching adapters to files based on file name or mime type
 */
use crate::adapters::*;

use failure::*;

use regex::{Regex, RegexSet};

use std::iter::Iterator;

use std::rc::Rc;

#[derive(Clone)]
pub enum FastMatcher {
    // MimeType(Regex),
    /**
     * without the leading dot, e.g. "jpg" or "tar.gz". Matched as /.*\.ext$/
     *
     */
    FileExtension(String),
    // todo: maybe add others, e.g. regex on whole filename or even paths
    // todo: maybe allow matching a directory (e.g. /var/lib/postgres)
}

#[derive(Clone)]
pub enum SlowMatcher {
    /// any type of fast matcher
    Fast(FastMatcher),
    ///
    /// match by exact mime type extracted using tree_magic
    /// TODO: allow match ignoring suffix etc?
    MimeType(String),
}

pub struct FileMeta {
    // filename is not actually a utf8 string, but since we can't do regex on OsStr and can't get a &[u8] from OsStr either,
    // and since we probably only want to do only matching on ascii stuff anyways, this is the filename as a string with non-valid bytes removed
    pub lossy_filename: String,
    // only given when slow matching is enabled
    pub mimetype: Option<String>,
}

pub fn extension_to_regex(extension: &str) -> Regex {
    Regex::new(&format!("\\.{}$", &regex::escape(extension))).expect("we know this regex compiles")
}

pub fn adapter_matcher<T: AsRef<str>>(
    adapter_names: &[T],
    slow: bool,
) -> Fallible<impl Fn(FileMeta) -> Option<(Rc<dyn FileAdapter>, SlowMatcher)>> {
    let adapters = get_adapters_filtered(adapter_names)?;
    // need order later
    let adapter_names: Vec<String> = adapters.iter().map(|e| e.metadata().name.clone()).collect();
    let mut fname_regexes = vec![];
    let mut mime_regexes = vec![];
    for adapter in adapters.into_iter() {
        let metadata = adapter.metadata();
        use SlowMatcher::*;
        for matcher in metadata.get_matchers(slow) {
            match matcher.as_ref() {
                MimeType(re) => {
                    mime_regexes.push((re.clone(), adapter.clone(), MimeType(re.clone())))
                }
                Fast(FastMatcher::FileExtension(re)) => fname_regexes.push((
                    extension_to_regex(re),
                    adapter.clone(),
                    Fast(FastMatcher::FileExtension(re.clone())),
                )),
            };
        }
    }
    let fname_regex_set = RegexSet::new(fname_regexes.iter().map(|p| p.0.as_str()))?;
    let mime_regex_set = RegexSet::new(mime_regexes.iter().map(|p| p.0.as_str()))?;
    Ok(move |meta: FileMeta| {
        let fname_matches: Vec<_> = fname_regex_set
            .matches(&meta.lossy_filename)
            .into_iter()
            .collect();
        let mime_matches: Vec<_> = if slow {
            mime_regex_set
                .matches(&meta.mimetype.expect("No mimetype?"))
                .into_iter()
                .collect()
        } else {
            vec![]
        };
        if fname_matches.len() + mime_matches.len() > 1 {
            // get first according to original priority list...
            // todo: kinda ugly
            let fa = fname_matches
                .iter()
                .map(|e| (fname_regexes[*e].1.clone(), fname_regexes[*e].2.clone()));
            let fb = mime_matches
                .iter()
                .map(|e| (mime_regexes[*e].1.clone(), mime_regexes[*e].2.clone()));
            let mut v = vec![];
            v.extend(fa);
            v.extend(fb);
            v.sort_by_key(|e| {
                adapter_names
                    .iter()
                    .position(|r| r == &e.0.metadata().name)
                    .expect("impossib7")
            });
            eprintln!(
                "Warning: found multiple adapters for {}:",
                meta.lossy_filename
            );
            for mmatch in v.iter() {
                eprintln!(" - {}", mmatch.0.metadata().name);
            }
            return Some(v[0].clone());
        }
        if mime_matches.is_empty() {
            if fname_matches.is_empty() {
                None
            } else {
                let (_, adapter, matcher) = &fname_regexes[fname_matches[0]];
                Some((adapter.clone(), matcher.clone()))
            }
        } else {
            let (_, adapter, matcher) = &mime_regexes[mime_matches[0]];
            Some((adapter.clone(), matcher.clone()))
        }
    })
}
