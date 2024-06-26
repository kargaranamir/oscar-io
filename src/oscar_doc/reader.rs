/*! OSCAR Schema v2 (22.01) Reader.

   Provides a way to read [Document]s from a [BufRead].

   TODO: Find a way to provide some reading of splitted corpora.
* !*/
#[cfg(feature = "avro")]
use avro_rs::Reader;
use flate2::bufread::MultiGzDecoder;
use log::info;
use std::{
    fs::File,
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
};

use crate::error::Error;

// use super::types::Document;
use crate::v3::Document;

/// Document reader.
/// The inner type has to implement [BufRead].
pub struct DocReader<R: BufRead> {
    r: R,
}

impl<R: BufRead> DocReader<R> {
    /// Create a new [DocReader].
    pub fn new(r: R) -> Self {
        DocReader { r }
    }
}

impl<R: BufRead> DocReader<BufReader<MultiGzDecoder<R>>> {
    pub fn from_gzip(r: R) -> Self {
        let dec = MultiGzDecoder::new(r);
        let br = BufReader::new(dec);
        DocReader::new(br)
    }
}

impl<R: BufRead> Iterator for DocReader<R> {
    type Item = Result<Document, Error>;

    /// Yields [Result]<[Document], [Error]>.
    /// Errors can be either [serde_json::Error] if the format is invalid, or [std::io::Error] if there has been some IO Error.
    fn next(&mut self) -> Option<Self::Item> {
        let mut s = String::new();
        match self.r.read_line(&mut s) {
            // stop if nothing is read
            Ok(0) => None,
            Ok(_) => {
                // Attempt to deserialize, map error to custom error enum if it fails
                let result: Result<Document, Error> =
                    serde_json::from_str(&s).map_err(|x| x.into());
                Some(result)
            }
            Err(e) => Some(Err(e.into())),
        }
    }
}

#[cfg(feature = "avro")]
pub struct AvroDocReader<'a, R> {
    r: Reader<'a, R>,
}

#[cfg(feature = "avro")]
impl<'a, R: Read> AvroDocReader<'a, R> {
    pub fn new(r: R) -> Self {
        let r = Reader::new(r).unwrap();
        Self { r }
    }
}

#[cfg(feature = "avro")]
impl<'a, R: Read> Iterator for AvroDocReader<'a, R> {
    type Item = Result<Document, Error>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.r.next() {
            // if we properly get a record, try to get document form it.
            // Otherwise, return error.
            Some(Ok(value)) => match avro_rs::from_value(&value) {
                Ok(d) => Some(Ok(d)),
                Err(e) => Some(Err(e.into())),
            },
            Some(Err(e)) => Some(Err(e.into())),
            None => None,
        }
    }
}

/// In the case where we have multiple splits for a given subcorpus.
pub struct SplitFileIter {
    //path to the directory
    //file names
    //counter
    base_path: PathBuf,
    file_name_start: String,
    file_name_end: String,
    file_name_extension: String,
    counter_start: usize,
    counter: usize,
    current_file: Option<DocReader<BufReader<File>>>,
}
impl SplitFileIter {
    pub fn new(
        base_path: PathBuf,
        file_name_start: &str,
        file_name_end: &str,
        file_name_extension: &str,
        counter_start: usize,
    ) -> SplitFileIter {
        SplitFileIter {
            base_path,
            file_name_start: file_name_start.to_string(),
            file_name_end: file_name_end.to_string(),
            file_name_extension: file_name_extension.to_string(),
            counter_start,
            counter: counter_start,
            current_file: None,
        }
    }

    pub fn rotate_file(&mut self) -> Result<(), Error> {
        let filename = self.file_name_start.to_owned()
            + &self.counter.to_string()
            + &self.file_name_end
            + &self.file_name_extension;
        let mut full_path = self.base_path.clone();
        full_path.push(filename);

        match File::open(full_path) {
            // everything is ok, we return a bufreader
            Ok(f) => {
                let br = BufReader::new(f);
                let dr = DocReader::new(br);
                self.counter += 1;
                self.current_file = Some(dr);
                Ok(())
            }

            // if the error is a NotFound, then we just arrived at the end
            // if not, there has been a problem.
            Err(e) => Err(e.into()),
        }
    }
}

// TODO: Check with gzipped
impl Iterator for SplitFileIter {
    // type Item = Result<BufReader<File>, Error>;
    type Item = Result<Document, Error>;

    /// Iterator on documents that is seamlessly iterating on file splits.
    /// **Currently uses some recursion and shouldn't (but may) recurse indefinitely.**
    fn next(&mut self) -> Option<Self::Item> {
        // check if current file is None or not
        match &mut self.current_file {
            // if current file is none, attempt to rotate file
            None => {
                match self.rotate_file() {
                    //if rotation went well, we can try again to call next()
                    // TODO: remove potential infinite recursion
                    Ok(()) => self.next(),

                    // if rotating went wrong, check if it's because of not found (end of split files) or other error
                    Err(e) => match e {
                        // if ioerror, check if it's because of a not found or not
                        Error::Io(ioerror) => {
                            // check not found condition and ensure that files have been rotated at least once
                            // or it could mean that the base provided path was not found.
                            if ioerror.kind() == std::io::ErrorKind::NotFound
                                && self.counter > self.counter_start
                            {
                                None

                            // If the error is not NotFound, return the error
                            } else {
                                Some(Err(ioerror.into()))
                            }
                        }

                        // return the error for non io errors
                        other => Some(Err(other)),
                    },
                }
            }

            // if there is an already opened file, get next document.
            // if next document is none (=EOF), close file by setting to None and
            // recursively call next.
            Some(file) => match file.next() {
                Some(doc_result) => Some(doc_result),
                None => {
                    // close file and try to open a new one
                    self.current_file = None;
                    self.next()
                }
            },
        }
    }
}

pub struct SplitFolderFileIter {
    current_file: Option<DocReader<BufReader<File>>>,
    files: Vec<PathBuf>,
    nb_files: usize,
    files_done: usize,
    //files: Box<dyn Iterator<Item = PathBuf>>,
}

impl SplitFolderFileIter {
    /// Create a new Self. If folder is a fie, the vector will be only populated with it.
    pub fn new(folder: &Path) -> Result<Self, Error> {
        // check if path is file
        // if it is, return a vec with only one path
        if folder.is_file() {
            Ok(Self {
                current_file: None,
                files: vec![folder.to_path_buf()],
                nb_files: 1,
                files_done: 0,
            })
        } else {
            match std::fs::read_dir(folder) {
                Ok(read_dir) => {
                    // read files (max-depth 1) and add them to vector
                    let mut files = vec![];
                    for dir in read_dir {
                        let dir = dir?.path();
                        if dir.is_file() {
                            files.push(dir);
                        }
                    }

                    if files.is_empty() {
                        return Err(Error::Custom(format!("No files found in {:?}", folder)));
                    }
                    // sort to be deterministic
                    files.sort_unstable();

                    // reverse so that it goes last...first
                    // and pop is more practical
                    files.reverse();
                    let nb_files = files.len();

                    Ok(Self {
                        current_file: None,
                        files,
                        nb_files,
                        files_done: 0,
                    })
                }
                Err(e) => Err(e.into()),
            }
        }
    }

    pub fn open_next_file(&mut self) -> Option<Result<(), Error>> {
        let next_file_path = self.files.pop();

        if let Some(next_file_path) = next_file_path {
            match File::open(next_file_path) {
                // everything is ok, we return a bufreader
                Ok(f) => {
                    let br = BufReader::new(f);
                    let dr = DocReader::new(br);
                    self.current_file = Some(dr);
                    self.files_done += 1;
                    info!("Reading file {}/{}", self.files_done, self.nb_files);
                    Some(Ok(()))
                }

                // if the error is a NotFound, then we just arrived at the end
                // if not, there has been a problem.
                Err(e) => Some(Err(e.into())),
            }
        } else {
            None
        }
    }
}

impl Iterator for SplitFolderFileIter {
    type Item = Result<Document, Error>;

    fn next(&mut self) -> Option<Self::Item> {
        match &mut self.current_file {
            // if current file is none, attempt to rotate file
            None => {
                match self.open_next_file() {
                    //if rotation went well, we can try again to call next()
                    // TODO: remove potential infinite recursion
                    Some(Ok(())) => self.next(),
                    Some(Err(e)) => Some(Err(e)),
                    None => None
                    // None => Some(Err(Error::Custom(
                    //     "Something went wrong when trying to open the first file..".to_string(),
                    // ))),
                }
            }

            // if there is an already opened file, get next document.
            // if next document is none (=EOF), close file by setting to None and
            // recursively call next.
            Some(file) => match file.next() {
                Some(doc_result) => Some(doc_result),
                None => {
                    // close file and try to open a new one
                    self.current_file = None;
                    self.next()
                }
            },
        }
    }
}

#[cfg(test)]
mod tests {

    use std::io::{BufReader, Cursor, Write};

    use super::DocReader;
    use crate::{error::Error, v3::Document};
    use flate2::{write::GzEncoder, Compression};

    fn get_samples() -> &'static str {
        r#"{"content":"this is the main content","warc_headers":{"warc-type":"conversion","warc-date":"2021-09-16T11:37:01Z","warc-refers-to":"<urn:uuid:3cc5dbf1-6932-44e3-a5f9-87bddb242ed1>","warc-block-digest":"sha1:AAN5C7C7I2JOXM5ZYB5YNFPRC5N6GJES","content-type":"text/plain","warc-target-uri":"http://accueil-enfants-d-un-meme-pere.be/","content-length":"5095","warc-identified-content-language":"fra,eng","warc-record-id":"<urn:uuid:7c1c010a-61ca-4383-92ba-008390a56fc9>"},"metadata":{"identification":{"label":"fr","prob":0.9586384},"annotation":["short_sentences","header","footer"],"sentence_identifications":[{"label": "fr", "prob": 0.9}]}}
{"content":"this is the main content","warc_headers":{"warc-type":"conversion","warc-date":"2021-09-16T11:37:01Z","warc-refers-to":"<urn:uuid:3cc5dbf1-6932-44e3-a5f9-87bddb242ed1>","warc-block-digest":"sha1:AAN5C7C7I2JOXM5ZYB5YNFPRC5N6GJES","content-type":"text/plain","warc-target-uri":"http://accueil-enfants-d-un-meme-pere.be/","content-length":"5095","warc-identified-content-language":"fra,eng","warc-record-id":"<urn:uuid:7c1c010a-61ca-4383-92ba-008390a56fc9>"},"metadata":{"identification":{"label":"fr","prob":0.9586384},"annotation":["short_sentences","header","footer"],"sentence_identifications":[{"label": "fr", "prob": 0.9}]}}
{"content":"this is the main content","warc_headers":{"warc-type":"conversion","warc-date":"2021-09-16T11:37:01Z","warc-refers-to":"<urn:uuid:3cc5dbf1-6932-44e3-a5f9-87bddb242ed1>","warc-block-digest":"sha1:AAN5C7C7I2JOXM5ZYB5YNFPRC5N6GJES","content-type":"text/plain","warc-target-uri":"http://accueil-enfants-d-un-meme-pere.be/","content-length":"5095","warc-identified-content-language":"fra,eng","warc-record-id":"<urn:uuid:7c1c010a-61ca-4383-92ba-008390a56fc9>"},"metadata":{"identification":{"label":"fr","prob":0.9586384},"annotation":["short_sentences","header","footer"],"sentence_identifications":[{"label": "fr", "prob": 0.9}]}}
{"content":"this is the main content","warc_headers":{"warc-type":"conversion","warc-date":"2021-09-16T11:37:01Z","warc-refers-to":"<urn:uuid:3cc5dbf1-6932-44e3-a5f9-87bddb242ed1>","warc-block-digest":"sha1:AAN5C7C7I2JOXM5ZYB5YNFPRC5N6GJES","content-type":"text/plain","warc-target-uri":"http://accueil-enfants-d-un-meme-pere.be/","content-length":"5095","warc-identified-content-language":"fra,eng","warc-record-id":"<urn:uuid:7c1c010a-61ca-4383-92ba-008390a56fc9>"},"metadata":{"identification":{"label":"fr","prob":0.9586384},"annotation":["short_sentences","header","footer"],"sentence_identifications":[{"label": "fr", "prob": 0.9}]}}
{"content":"this is the main content","warc_headers":{"warc-type":"conversion","warc-date":"2021-09-16T11:37:01Z","warc-refers-to":"<urn:uuid:3cc5dbf1-6932-44e3-a5f9-87bddb242ed1>","warc-block-digest":"sha1:AAN5C7C7I2JOXM5ZYB5YNFPRC5N6GJES","content-type":"text/plain","warc-target-uri":"http://accueil-enfants-d-un-meme-pere.be/","content-length":"5095","warc-identified-content-language":"fra,eng","warc-record-id":"<urn:uuid:7c1c010a-61ca-4383-92ba-008390a56fc9>"},"metadata":{"identification":{"label":"fr","prob":0.9586384},"annotation":["short_sentences","header","footer"],"sentence_identifications":[{"label": "fr", "prob": 0.9}]}}"#
    }

    #[test]
    fn test_read_simple() {
        let content = get_samples();
        let mut r = DocReader::new(content.as_bytes());
        for _ in 0..5 {
            assert!(r.next().is_some());
        }
        assert!(r.next().is_none());
    }

    #[test]
    fn test_bad_format() {
        let content = r#"{"foo": "bar"}"#;
        let mut r = DocReader::new(content.as_bytes());
        match r.next() {
            Some(Err(Error::SerdeJson(_))) => assert!(true),
            x => panic!("wrong return: {:?}", x),
        }
    }

    #[test]
    fn test_compressed_data() {
        let content = get_samples();

        //create uncompressed data
        let br = BufReader::new(content.as_bytes());
        let r = DocReader::new(br);
        let documents: Result<Vec<Document>, Error> = r.collect();

        //create compressed data
        let mut compressed_content = vec![];
        {
            let mut enc = GzEncoder::new(&mut compressed_content, Compression::fast());
            enc.write(content.as_bytes()).unwrap();
        }

        let c = Cursor::new(&mut compressed_content);
        let r = DocReader::from_gzip(c);
        let documents_from_compressed: Result<Vec<Document>, Error> = r.collect();

        assert_eq!(documents.unwrap(), documents_from_compressed.unwrap())
    }
}
