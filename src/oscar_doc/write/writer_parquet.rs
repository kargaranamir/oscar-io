//! Parquet corpus writer
//!
//! The parquet files might not follow the hierarchy of usual jsonl files.
//! See [here](https://github.com/apache/parquet-format/blob/master/LogicalTypes.md) for parquet logicaltypes
//! record_id ->

use std::{
    collections::HashMap,
    io::{Seek, Write},
    sync::Arc,
};

use crate::oscar_doc::types::Document;
use crate::{common::Identification, error::Error};
use lazy_static::lazy_static;
use parquet::{
    data_type::ByteArray,
    file::{properties::WriterProperties, writer::SerializedFileWriter},
    schema::{parser::parse_message_type, types::Type},
};
// const DOCUMENT_SCHEMA: &'static str = "
//         message document {
//             REQUIRED BYTE_ARRAY content (UTF8);
//             REQUIRED group warc_headers (MAP) {
//                 required binary header (UTF8);
//                 required binary value (UTF8);
//             }
//             required group metadata {
//                 required group identification {
//                     required binary lang (UTF8);
//                     required float id;
//                 }
//                 required group annotations (LIST) {
//                     repeated group list {
//                         optional binary annotation (UTF8);
//                     }
//                 }
//                 required group sentence_identifications (LIST) {
//                     repeated group list {
//                         required binary lang (UTF8);
//                         required float id;
//                     }
//                 }
//             }
//         }
//         ";

// const DOCUMENT_SCHEMA: &'static str = r#"
// message document {
//     required group sentences (LIST) {
//         repeated group list {
//             optional group element {
//                 required binary sentence (UTF8);
//                 optional group identification {
//                     required binary label (UTF8);
//                     required float prob;
//                 }
//             }
//         }
//     }
// }"#;

const DOCUMENT_SCHEMA: &'static str = r#"
message document {
    required group lines (LIST) {
        repeated group list {
            required binary sentence (UTF8);
        }
        required binary label (UTF8);
        required float prob;
    }
}"#;

lazy_static! {
    #[derive(Debug)]
    pub static ref SCHEMA: Type = parse_message_type(DOCUMENT_SCHEMA).expect("invalid schema");
}

pub struct ParquetWriter<W: Write + Seek> {
    writer: SerializedFileWriter<W>,
}

impl<W: Write + Seek> ParquetWriter<W> {
    pub fn new(writer: W, props: WriterProperties) -> Result<Self, parquet::errors::ParquetError> {
        Ok(Self {
            writer: SerializedFileWriter::new(writer, Arc::new(SCHEMA.clone()), Arc::new(props))?,
        })
    }

    pub fn write_docs(&mut self, docs: &[Document]) -> Result<(), Error> {
        let doc_grouped = DocGroup::new(docs);

        // iterate on each column and write
        todo!()
    }
}

#[derive(Debug)]
struct DocGroup<'a> {
    contents: Vec<&'a str>,
    warc_headers: Vec<&'a HashMap<String, String>>,
    annotations: Vec<&'a Option<Vec<String>>>,
    ids: Vec<&'a Identification>,
    line_ids: Vec<&'a [Option<Identification>]>,
    nb_col: usize,
}

impl<'a> DocGroup<'a> {
    pub fn new(docs: &'a [Document]) -> Self {
        let mut contents = Vec::new();
        let mut warc_headers = Vec::new();
        let mut annotations = Vec::new();
        let mut ids = Vec::new();
        let mut line_ids = Vec::new();
        for d in docs {
            contents.push(d.content().as_str());
            warc_headers.push(d.warc_headers());
            annotations.push(d.metadata().annotation());
            ids.push(d.metadata().identification());
            line_ids.push(d.metadata().sentence_identifications());
        }

        Self {
            contents,
            warc_headers,
            annotations,
            ids,
            line_ids,
            nb_col: 0,
        }
    }
}

struct ParquetColumn<T> {
    vals: Vec<T>,
    def_levels: Vec<i16>,
    rep_levels: Vec<i16>,
}
/// simple, to experiment with parquet
struct SimpleDocGroup<'a> {
    contents: Vec<Vec<ByteArray>>,               //lines
    annotations: Vec<&'a Option<Vec<String>>>,   //annotations (or lack thereof)
    line_ids: Vec<&'a [Option<Identification>]>, //
    nb_col: usize,
}
// impl<'a> Iterator for DocGroup<'a> {
//     type Item = DocGroupPart<'a>;

//     fn next(&mut self) -> Option<Self::Item> {
//         match self.nb_col {
//             0 => Some(DocGroupPart::Contents(&self.contents)),
//             1 => Some(DocGroupPart::Warcs(&self.warc_headers)),
//             2 => Some(DocGroupPart::Annotations(&self.annotations)),
//             3 => Some(DocGroupPart::Id(&self.ids)),
//             4 => Some(DocGroupPart::LineIds(&self.line_ids)),
//             _ => None,
//         }
//     }
// }
struct DocumentFieldsIterator<'a> {
    inner: &'a Document,
    part_nb: usize,
}

#[derive(Debug, PartialEq)]
enum DocPart<'a> {
    Content(&'a str),
    Warc(&'a HashMap<String, String>),
    Annotation(&'a Option<Vec<String>>),
    Id(&'a Identification),
    LineIds(&'a [Option<Identification>]),
}

enum DocGroupPart<'a> {
    Contents(&'a Vec<&'a str>),
    Warcs(&'a Vec<&'a HashMap<String, String>>),
    Annotations(&'a Vec<&'a Option<Vec<String>>>),
    Id(&'a Vec<&'a Identification>),
    LineIds(&'a Vec<&'a [Option<Identification>]>),
}
impl<'a> Iterator for DocumentFieldsIterator<'a> {
    type Item = DocPart<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        let ret = match self.part_nb {
            0 => Some(DocPart::Content(self.inner.content())),
            1 => Some(DocPart::Warc(self.inner.warc_headers())),
            2 => Some(DocPart::Annotation(self.inner.metadata().annotation())),
            3 => Some(DocPart::Id(self.inner.identification())),
            4 => Some(DocPart::LineIds(
                self.inner.metadata().sentence_identifications(),
            )),
            _ => None,
        };
        self.part_nb += 1;
        ret
    }
}
impl Document {
    fn iter_parquet(&self) -> DocumentFieldsIterator {
        DocumentFieldsIterator {
            inner: &self,
            part_nb: 0,
        }
    }
}

#[cfg(test)]
mod test_writer {
    use std::{collections::HashMap, fs::File, sync::Arc};

    use parquet::{
        column::writer::ColumnWriter,
        data_type::ByteArray,
        file::{
            properties::WriterProperties,
            writer::{FileWriter, InMemoryWriteableCursor, SerializedFileWriter},
        },
        schema::{parser::parse_message_type, types::Type},
    };

    use crate::{
        common::Identification,
        error::Error,
        lang::Lang,
        oscar_doc::{write::writer_parquet::SCHEMA, Document, Metadata},
    };

    use super::{ParquetWriter, DOCUMENT_SCHEMA};

    #[test]
    fn test_simple() {
        let schema = r#"
        message identifications {
            repeated group idents {
                required binary id (UTF8);
                optional float prob;
            }
        }"#;
        let schema = r#"
        message identifications {
            repeated group idents (LIST) {
                repeated group list {
                    optional binary element (UTF8);
                  }
            }
        }"#;

        let (ids, probs) = {
            let test_id = Identification::new(crate::lang::Lang::Af, 0.1);
            let ids = vec![test_id.label().to_string().as_str().into(); 100];
            let probs = vec![*test_id.prob(); 100];
            (ids, probs)
        };
        let schema = parse_message_type(schema).unwrap();
        print_arbo(&schema, 2);

        let buf = File::create("./test.parquet").unwrap();
        let props = WriterProperties::builder().build();
        let mut w =
            SerializedFileWriter::new(buf, Arc::new(schema.clone()), Arc::new(props)).unwrap();

        let mut rg = w.next_row_group().unwrap();
        while let Some(mut col_writer) = rg.next_column().unwrap() {
            if let ColumnWriter::ByteArrayColumnWriter(ref mut a) = col_writer {
                println!("writing strings");
                let def_levels = vec![1; 100];
                let rep_levels = vec![0; 100];
                a.write_batch(&ids, Some(&def_levels), Some(&rep_levels))
                    .unwrap();

                println!("def id: {def_levels:?}");
                println!("rep id: {rep_levels:?}");
                //write ids
            } else if let ColumnWriter::FloatColumnWriter(ref mut a) = col_writer {
                println!("writing floats");
                let def_levels = vec![2; 100];
                let mut rep_levels = vec![0; 99];
                rep_levels.push(0);
                rep_levels.reverse();
                a.write_batch(&probs, Some(&def_levels), Some(&rep_levels))
                    .unwrap();
                println!("def prob: {def_levels:?}");
                println!("rep prob: {rep_levels:?}");
                //write floats
            }
            rg.close_column(col_writer).unwrap();
        }
        w.close_row_group(rg).unwrap();
        w.close().unwrap();
    }

    #[test]
    fn test_document() {
        let content = "A nos enfants de la patrie
        Le jour de gloire est arrivé
        This is the French national anthem
        xxyyxyxyxxyxyxyxyxxy";

        let ids = vec![
            Some(Identification::new(Lang::Fr, 1.0)),
            Some(Identification::new(Lang::Fr, 1.0)),
            Some(Identification::new(Lang::En, 1.0)),
            None,
        ];
        let metadata = Metadata::new(
            &Identification::new(Lang::Fr, 0.8),
            &Some(vec!["adult".to_string()]),
            &ids,
        );
        let d = Document::new(content.to_string(), HashMap::new(), metadata);

        let schema = parse_message_type(DOCUMENT_SCHEMA).unwrap();
        print_arbo(&schema, 2);

        let buf = File::create("./test.parquet").unwrap();
        let props = WriterProperties::builder().build();
        let mut w =
            SerializedFileWriter::new(buf, Arc::new(schema.clone()), Arc::new(props)).unwrap();

        let mut rg = w.next_row_group().unwrap();
        let mut nb_col = 0;
        while let Some(mut col_writer) = rg.next_column().unwrap() {
            match col_writer {
                ColumnWriter::BoolColumnWriter(ref mut c) => println!("bool"),
                ColumnWriter::Int32ColumnWriter(ref mut c) => println!("int32"),
                ColumnWriter::Int64ColumnWriter(ref mut c) => println!("int64"),
                ColumnWriter::Int96ColumnWriter(ref mut c) => println!("int96"),
                ColumnWriter::FloatColumnWriter(ref mut c) => {
                    let probs = d.metadata().sentence_identifications();
                    // .iter()
                    // .map(|x| match x {
                    //     Some(id) => id.prob(),
                    //     None => &0.0,
                    // })
                    // .collect();
                    let def_levels: Vec<i16> = probs
                        .iter()
                        .map(|x| if x.is_none() { 1 } else { 2 })
                        .collect();
                    let rep_levels = vec![1; probs.len()];

                    let probs: Vec<f32> = probs
                        .iter()
                        .map(|x| match x {
                            Some(id) => *id.prob(),
                            None => 0.0,
                        })
                        .collect();

                    c.write_batch(&probs, Some(&def_levels), Some(&rep_levels))
                        .unwrap();
                }
                ColumnWriter::DoubleColumnWriter(ref mut c) => println!("double"),
                ColumnWriter::ByteArrayColumnWriter(ref mut c) => match nb_col {
                    0 => {
                        let lines: Vec<ByteArray> = d.content().lines().map(|x| x.into()).collect();
                        //all defined
                        let def_levels = vec![1; lines.len()];
                        //first one creates a new nesting
                        let mut rep_levels = vec![1; lines.len() - 1];
                        rep_levels.push(0);
                        rep_levels.reverse();

                        c.write_batch(&lines, Some(&def_levels), Some(&rep_levels))
                            .unwrap();

                        nb_col += 1;
                    }
                    1 => {
                        let labels: Vec<ByteArray> = d
                            .metadata()
                            .sentence_identifications()
                            .iter()
                            .map(|x| match x {
                                Some(id) => id.label().to_string().as_str().into(),
                                None => ByteArray::new(),
                            })
                            .collect();
                        // definition is 2 if label is not None
                        let def_levels: Vec<_> = labels
                            .iter()
                            .map(|x| if x.is_empty() { 2 } else { 1 })
                            .collect();
                        let rep_levels = vec![2; labels.len()];

                        c.write_batch(&labels, Some(&def_levels), Some(&rep_levels))
                            .unwrap();
                        nb_col += 1;
                    }
                    _ => panic!("wrong col type"),
                },
                ColumnWriter::FixedLenByteArrayColumnWriter(ref mut c) => println!("flbytearray"),
            }
            rg.close_column(col_writer).unwrap();
        }
        w.close_row_group(rg).unwrap();
        w.close().unwrap();
    }
    #[test]
    fn test_simple_list() {
        let schema = r#"
        message identifications {
            repeated group idents {
                required binary id (UTF8);
                optional float prob;
            }
        }"#;
        let schema = r#"
        message identifications {
            required group idents (LIST) {
                repeated group list {
                    optional group element {
                        required binary id (UTF8);
                        required float prob;
                    }
                  }
            }
        }"#;

        let (ids, probs) = {
            let test_id = Identification::new(crate::lang::Lang::Af, 0.1);
            let ids = vec![test_id.label().to_string().as_str().into(); 100];
            let probs = vec![*test_id.prob(); 100];
            (ids, probs)
        };
        let schema = parse_message_type(schema).unwrap();
        print_arbo(&schema, 2);

        let buf = File::create("./test.parquet").unwrap();
        let props = WriterProperties::builder().build();
        let mut w =
            SerializedFileWriter::new(buf, Arc::new(schema.clone()), Arc::new(props)).unwrap();

        let mut rg = w.next_row_group().unwrap();
        while let Some(mut col_writer) = rg.next_column().unwrap() {
            if let ColumnWriter::ByteArrayColumnWriter(ref mut a) = col_writer {
                println!("writing strings");
                let def_levels = vec![2; 100];
                let rep_levels = vec![0; 100];
                a.write_batch(&ids, Some(&def_levels), Some(&rep_levels))
                    .unwrap();

                println!("def id: {def_levels:?}");
                println!("rep id: {rep_levels:?}");
                //write ids
            } else if let ColumnWriter::FloatColumnWriter(ref mut a) = col_writer {
                println!("writing floats");
                let def_levels = vec![2; 100];
                let mut rep_levels = vec![0; 99];
                rep_levels.push(0);
                rep_levels.reverse();
                a.write_batch(&probs, Some(&def_levels), Some(&rep_levels))
                    .unwrap();
                println!("def prob: {def_levels:?}");
                println!("rep prob: {rep_levels:?}");
                //write floats
            }
            rg.close_column(col_writer).unwrap();
        }
        w.close_row_group(rg).unwrap();
        w.close().unwrap();
    }
    #[test]
    fn test_id_auto() {
        // sentence identifications for a single document
        type SentenceId = Vec<Option<Identification>>;
        // sentence identifications for n documents
        type SentenceIds = Vec<SentenceId>;

        let sentence_ids = vec![
            Some(Identification::new(Lang::En, 1.0)),
            Some(Identification::new(Lang::Fr, 0.1)),
            Some(Identification::new(Lang::Az, 0.2)),
            None,
            Some(Identification::new(Lang::Am, 0.3)),
            None,
            Some(Identification::new(Lang::Bar, 0.4)),
            Some(Identification::new(Lang::Zh, 0.5)),
        ];

        let def_expected = [1, 1, 1, 0, 1, 0, 1, 1];
        let rep_expected = [0, 1, 1, 1, 1, 1, 1, 1];
        dbg!(&sentence_ids);
        auto_ser(&sentence_ids, 0);
        fn auto_ser(sentenceids: &SentenceId, default_level: u16) -> Result<(), Error> {
            //def: level where it's null
            //rep: level at which we have to create a new list
            let mut def_levels = Vec::with_capacity(sentenceids.len());
            let mut rep_levels = Vec::with_capacity(sentenceids.len());

            let mut ids: Vec<Option<ByteArray>> = Vec::with_capacity(sentenceids.len());
            let mut probs = Vec::with_capacity(sentenceids.len());

            let mut sid_iter = sentenceids.iter();

            //first step
            match sid_iter.next() {
                None => panic!("empty"),
                Some(None) => {
                    ids.push(None);
                    probs.push(None);

                    def_levels.push(default_level);
                    rep_levels.push(default_level);
                }
                Some(Some(sid)) => {
                    ids.push(Some(sid.label().to_string().as_str().into()));
                    probs.push(Some(sid.prob()));
                    def_levels.push(default_level + 1);
                    rep_levels.push(default_level);
                }
            }
            for sid in sid_iter {
                match sid {
                    None => {
                        ids.push(None);
                        probs.push(None);

                        def_levels.push(default_level + 1);
                        rep_levels.push(default_level);
                    }
                    Some(sid) => {
                        ids.push(Some(sid.label().to_string().as_str().into()));
                        probs.push(Some(sid.prob()));
                        def_levels.push(default_level + 1);
                        rep_levels.push(default_level + 1);
                    }
                }
            }

            println!("def: {def_levels:?}");
            println!("rep: {rep_levels:?}");
            println!("{ids:?}");
            println!("{probs:?}");
            Ok(())
        }
        //todo: go from Vec<Option<Identification>>, and have something automatically setting def and rep levels.
        //hint: you can use a param "base id level" to be able to use -1/-2 for each thingy
    }

    #[test]
    fn test_tiny_nested() {
        let schema = r#"
            message metadata {
                repeated group sentence_identifications {
                        required binary lang (UTF8);
                        required float prob;
                }
            }
        "#;

        // lang
        //   DEF: 1 (required)
        //   REP: 1 (sentence_ids is repeated)
        // prob
        //   DEF: 1 (required)
        //   REP: 1 (sentence_ids is repeated)
        let schema = parse_message_type(schema).unwrap();
        print_arbo(&schema, 2);
        let buf = InMemoryWriteableCursor::default();
        let mut buf = File::create("./test.parquet").unwrap();
        let props = WriterProperties::builder().build();
        let mut w =
            SerializedFileWriter::new(buf, Arc::new(schema.clone()), Arc::new(props)).unwrap();

        let mut rg = w.next_row_group().unwrap();
        while let Some(mut col_writer) = rg.next_column().unwrap() {
            match col_writer {
                parquet::column::writer::ColumnWriter::BoolColumnWriter(ref mut a) => {
                    println!("bool")
                }
                parquet::column::writer::ColumnWriter::Int32ColumnWriter(ref mut a) => {
                    println!("int32")
                }
                parquet::column::writer::ColumnWriter::Int64ColumnWriter(_) => println!(),
                parquet::column::writer::ColumnWriter::Int96ColumnWriter(_) => println!(),
                parquet::column::writer::ColumnWriter::FloatColumnWriter(ref mut a) => {
                    // prob
                    println!("float");
                    let values: Vec<_> = (0..100i32).map(|x| x as f32).collect();
                    a.write_batch(&values, Some(&[1; 100]), Some(&[1; 100]))
                        .unwrap();
                }
                parquet::column::writer::ColumnWriter::DoubleColumnWriter(_) => println!("double"),
                parquet::column::writer::ColumnWriter::ByteArrayColumnWriter(ref mut a) => {
                    println!("bytearray");

                    // build strings, get view as str and into bytearray
                    let strs: Vec<_> = (0..100i32)
                        .map(|i| format!("lang_{i}").as_str().into())
                        .collect();
                    // let strs_borrow = strs.iter().collect();
                    let mut def_levels = vec![0; 1];
                    def_levels.append(&mut vec![1; 99]);
                    a.write_batch(&strs, Some(&def_levels), Some(&[1; 100]))
                        .unwrap();
                }
                parquet::column::writer::ColumnWriter::FixedLenByteArrayColumnWriter(_) => {
                    println!("fixedlenbytearray")
                }
            }
            rg.close_column(col_writer).unwrap();
        }
        w.close_row_group(rg).unwrap();
        w.close().unwrap();
    }
    #[test]
    fn test_simple_write() {
        let buf = InMemoryWriteableCursor::default();
        let w = ParquetWriter::new(buf, WriterProperties::builder().build()).unwrap();

        print_arbo(&*SCHEMA, 0);
    }

    fn print_arbo(node: &Type, indent: usize) {
        println!(
            "{}{} (cvt_type: {:?}, logic_type: {:?})",
            vec![" "; indent].join(""),
            node.name(),
            node.get_basic_info().converted_type(),
            node.get_basic_info().logical_type()
        );
        if let Type::GroupType {
            basic_info: _,
            fields: fields,
        } = node
        {
            for sub_node in fields {
                print_arbo(sub_node, indent + 4);
            }
        }
    }
}
#[cfg(test)]
mod test_doc_group {
    use std::collections::HashMap;

    use crate::oscar_doc::{Document, Metadata};

    use super::DocGroup;

    #[test]
    fn from_vec() {
        let docs: Vec<Document> = ["hello", "second document", "third document\n :)"]
            .into_iter()
            .map(|content| Document::new(content.to_string(), HashMap::new(), Metadata::default()))
            .collect();

        let docgroup = DocGroup::new(&docs);
        println!("{docgroup:#?}");
    }
}
#[cfg(test)]
mod test_doc_iter {
    use std::collections::HashMap;

    use crate::{
        common::Identification,
        lang::Lang,
        oscar_doc::{write::writer_parquet::DocPart, Document, Metadata},
    };

    #[test]
    fn foo() {
        let d = Document::new("hello!".to_string(), HashMap::new(), Metadata::default());
        let mut d_iter = d.iter_parquet();
        assert_eq!(d_iter.next(), Some(DocPart::Content(&"hello!".to_string())));
        assert_eq!(d_iter.next(), Some(DocPart::Warc(&HashMap::new())));
        assert_eq!(d_iter.next(), Some(DocPart::Annotation(&None)));
        assert_eq!(
            d_iter.next(),
            Some(DocPart::Id(&Identification::new(Lang::En, 1.0)))
        );
        assert_eq!(
            d_iter.next(),
            Some(DocPart::LineIds(&vec![Some(Identification::new(
                Lang::En,
                1.0
            ))]))
        );
    }
}
