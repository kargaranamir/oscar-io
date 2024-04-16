# oscar-io
Types and IO (Reader/Writer) for OSCAR Corpus processing and generation.

The crate provides basic abstractions around Corpus items and generic readers/writers useable in OSCAR Corpus files.
At some time, it should replace reader implementations in both [Ungoliant](https://github.com/kargaranamir/Ungoliant) and [oscar-tools](https://github.com/kargaranamir/oscar-tools).


## Features

`oscar-io` aims to provide readers/writers for numerous types of OSCAR Corpora.


### OSCAR v2
- Reader 
    - [x] Uncompressed [oscar_doc::Reader::new]
    - [x] GZipped [oscar_doc::Reader::from_gzip]
    - [ ] Parquet
- Writer
    - [x] Uncompressed [oscar_doc::Writer::new]
    - [ ] GZipped [oscar_doc::Writer::new] (using a [GzEncoder] reader, `from_gzip` not yet implemented)
    - [ ] Parquet
- SplitReader (Should be unified with SplitReader with `split_size: Option<u64>`)
    - [x] Uncompressed
    - [ ] GZipped
- SplitWriter (Same)
    - [ ] Uncompressed
    - [ ] GZipped

### OSCAR v1.1
- [ ] Reader
- [ ] Writer
- [ ] SplitReader (Should be unified with SplitReader with `split_size: Option<u64>`)
- [ ] SplitWriter (Same)

### OSCAR v1
- [ ] Reader
- [ ] Writer
- [ ] SplitReader
- [ ] SplitWriter
