use std::{
    io::{BufRead, BufReader, Lines, Read},
    ops::Range,
};

use crate::ReaderError;

#[derive(Debug)]
pub struct Reader<R> {
    lines: Lines<BufReader<R>>,
    widths: Vec<usize>,
    separator_length: usize,
    flexible_width: bool,
    header: Option<Record>,
}

impl<R> Reader<R>
where
    R: Read,
{
    pub fn new(
        reader: R,
        widths: Vec<usize>,
        separator_length: usize,
        flexible_width: bool,
        has_header: bool,
    ) -> Result<Self, ReaderError> {
        let mut lines = BufReader::new(reader).lines();
        let header = {
            if has_header {
                let line = lines.next().ok_or(ReaderError::EmptyLine)??;
                Some(Record::try_new(
                    line,
                    &widths,
                    separator_length,
                    flexible_width,
                )?)
            } else {
                None
            }
        };
        Ok(Self {
            lines,
            widths,
            separator_length,
            flexible_width,
            header,
        })
    }

    pub fn header(&self) -> Option<Record> {
        self.header.clone()
    }

    pub fn records(self) -> RecordIter<R> {
        RecordIter { reader: self }
    }
}

#[derive(Debug)]
pub struct RecordIter<R> {
    reader: Reader<R>,
}

impl<R> RecordIter<R> {
    pub fn header(&self) -> Option<Record> {
        self.reader.header.clone()
    }
}

impl<R> Iterator for RecordIter<R>
where
    R: Read,
{
    type Item = Result<Record, ReaderError>;

    fn next(&mut self) -> Option<Self::Item> {
        self.reader.lines.next().map(|result| {
            Record::try_new(
                result?,
                &self.reader.widths,
                self.reader.separator_length,
                self.reader.flexible_width,
            )
        })
    }
}

#[derive(Debug, Clone)]
pub struct Record {
    line: String,
    ranges: Vec<Range<usize>>,
}

impl Record {
    pub fn try_new(
        line: String,
        widths: &[usize],
        sep_len: usize,
        flexible_widths: bool,
    ) -> Result<Self, ReaderError> {
        if line.is_empty() {
            Err(ReaderError::EmptyLine)
        } else {
            let mut start = 0;
            let ranges = widths
                .iter()
                .copied()
                .map(|w| {
                    let rem = line.len() - start;
                    match rem.cmp(&w) {
                        std::cmp::Ordering::Less => {
                            if flexible_widths {
                                let rng = start..line.len();
                                start = line.len();
                                Ok(rng)
                            } else {
                                let err = ReaderError::WidthMismatch(start, w);
                                start = line.len();
                                Err(err)
                            }
                        }
                        std::cmp::Ordering::Equal => {
                            let rng = start..line.len();
                            start = line.len();
                            Ok(rng)
                        }
                        std::cmp::Ordering::Greater => line[start..]
                            .char_indices()
                            .nth(w)
                            .map(|(i, _)| {
                                let end = start + i;
                                let rng = start..end;
                                start = end + sep_len;
                                rng
                            })
                            .ok_or(ReaderError::WidthMismatch(start, w)),
                    }
                })
                .collect::<Result<Vec<_>, ReaderError>>()?;
            Ok(Self { line, ranges })
        }
    }
    pub fn get(&self, index: usize) -> Option<&str> {
        self.ranges
            .get(index)
            .cloned()
            .and_then(|range| self.line.get(range))
    }

    pub fn iter(&self) -> FwrFieldIter {
        FwrFieldIter { fwr: self, next: 0 }
    }
}

#[derive(Debug, Clone)]
pub struct FwrFieldIter<'a> {
    fwr: &'a Record,
    next: usize,
}

impl<'a> Iterator for FwrFieldIter<'a> {
    type Item = &'a str;

    fn next(&mut self) -> Option<Self::Item> {
        self.fwr.get(self.next).map(|slice| {
            self.next += 1;
            slice
        })
    }
}

#[cfg(test)]
mod tests {

    use rand::distributions::Alphanumeric;
    use rand::thread_rng;
    use rand::Rng;

    use super::*;
    use std::fs::File;
    use std::io::Cursor;
    use std::io::Write;
    use std::path::PathBuf;

    fn create_test_file(content: &str) -> Result<PathBuf, Box<dyn std::error::Error>> {
        let file_path = PathBuf::from(format!(
            "test_fwf_file_{}.txt",
            thread_rng()
                .sample_iter(Alphanumeric)
                .take(16)
                .map(char::from)
                .collect::<String>()
        ));
        let mut file = File::create(&file_path)?;
        write!(file, "{}", content)?;
        Ok(file_path)
    }

    fn delete_test_file(path: &PathBuf) -> Result<(), Box<dyn std::error::Error>> {
        std::fs::remove_file(path)?;
        Ok(())
    }

    #[test]
    fn test_fwf_file_reader_with_header() {
        let content = "header1header2header3\n123456789\n987    654    321    \n";
        let file_path = create_test_file(content).unwrap();
        let widths = vec![7, 7, 7];

        let reader = Reader::new(
            File::open(file_path.clone()).unwrap(),
            widths,
            0,
            false,
            true,
        )
        .unwrap();

        let header = reader.header().clone().unwrap();
        assert_eq!(header.get(0), Some("header1"));
        assert_eq!(header.get(1), Some("header2"));
        assert_eq!(header.get(2), Some("header3"));

        let mut records = reader.records();
        assert!(matches!(
            records.next().unwrap().unwrap_err(),
            ReaderError::WidthMismatch(7, 7)
        ));

        let record2 = records.next().unwrap().unwrap();
        assert_eq!(record2.get(0), Some("987    "));
        assert_eq!(record2.get(1), Some("654    "));
        assert_eq!(record2.get(2), Some("321    "));

        assert!(records.next().is_none());

        delete_test_file(&file_path).unwrap();
    }

    #[test]
    fn test_fwf_file_reader_without_header() {
        let content = "123456789\n987654321\n";
        let file_path = create_test_file(content).unwrap();
        let widths = vec![3, 3, 3];

        let reader = Reader::new(
            File::open(file_path.clone()).unwrap(),
            widths,
            0,
            false,
            false,
        )
        .unwrap();

        let header = reader.header();
        assert!(header.is_none());

        let mut records = reader.records();
        let record1 = records.next().unwrap().unwrap();
        assert_eq!(record1.get(0), Some("123"));
        assert_eq!(record1.get(1), Some("456"));
        assert_eq!(record1.get(2), Some("789"));

        let record2 = records.next().unwrap().unwrap();
        assert_eq!(record2.get(0), Some("987"));
        assert_eq!(record2.get(1), Some("654"));
        assert_eq!(record2.get(2), Some("321"));

        assert!(records.next().is_none());

        delete_test_file(&file_path).unwrap();
    }

    #[test]
    fn test_fwf_file_reader_with_separator() {
        let content = "123-456-789\n987-654-321\n";
        let file_path = create_test_file(content).unwrap();
        let widths = vec![3, 3, 3];

        let reader = Reader::new(
            File::open(file_path.clone()).unwrap(),
            widths,
            1,
            false,
            false,
        )
        .unwrap();

        let mut records = reader.records();
        let record1 = records.next().unwrap().unwrap();
        assert_eq!(record1.get(0), Some("123"));
        assert_eq!(record1.get(1), Some("456"));
        assert_eq!(record1.get(2), Some("789"));

        let record2 = records.next().unwrap().unwrap();
        assert_eq!(record2.get(0), Some("987"));
        assert_eq!(record2.get(1), Some("654"));
        assert_eq!(record2.get(2), Some("321"));

        assert!(records.next().is_none());

        delete_test_file(&file_path).unwrap();
    }

    #[test]
    fn test_fwf_file_reader_with_flexible_width() {
        let content = "123456\n987654321\n";
        let file_path = create_test_file(content).unwrap();
        let widths = vec![3, 3, 3];

        let reader = Reader::new(
            File::open(file_path.clone()).unwrap(),
            widths,
            0,
            true,
            false,
        )
        .unwrap();

        let mut records = reader.records();
        let record1 = records.next().unwrap().unwrap();
        assert_eq!(record1.get(0), Some("123"));
        assert_eq!(record1.get(1), Some("456"));
        assert_eq!(record1.get(2), Some("")); // Last field is missing

        let record2 = records.next().unwrap().unwrap();
        assert_eq!(record2.get(0), Some("987"));
        assert_eq!(record2.get(1), Some("654"));
        assert_eq!(record2.get(2), Some("321"));

        assert!(records.next().is_none());

        delete_test_file(&file_path).unwrap();
    }

    #[test]
    fn test_fwf_file_reader_empty_file() {
        let content = "";
        let file_path = create_test_file(content).unwrap();
        let widths = vec![3, 3, 3];

        let reader = Reader::new(
            File::open(file_path.clone()).unwrap(),
            widths,
            0,
            false,
            false,
        )
        .unwrap();

        let header = reader.header();
        assert!(header.is_none());

        let records = reader.records();
        assert!(records.into_iter().next().is_none());

        delete_test_file(&file_path).unwrap();
    }

    #[test]
    fn test_fwf_record_iter_basic() {
        let data = "123456789\n987654321\n".as_bytes();
        let lines = BufReader::new(Cursor::new(data)).lines();
        let widths = vec![3, 3, 3];
        let separator_length = 0;
        let flexible_width = false;

        let mut iter = RecordIter {
            reader: Reader {
                lines,
                widths,
                separator_length,
                flexible_width,
                header: None,
            },
        };

        let record1 = iter.next().unwrap().unwrap();
        assert_eq!(record1.line, "123456789");
        assert_eq!(record1.ranges, vec![0..3, 3..6, 6..9]);

        let record2 = iter.next().unwrap().unwrap();
        assert_eq!(record2.line, "987654321");
        assert_eq!(record2.ranges, vec![0..3, 3..6, 6..9]);

        assert!(iter.next().is_none());
    }

    #[test]
    fn test_fwf_record_iter_with_flexible_width() {
        let data = "123456\n987654321\n".as_bytes();
        let lines = BufReader::new(Cursor::new(data)).lines();
        let widths = vec![3, 3, 3];
        let separator_length = 0;
        let flexible_width = true;

        let mut iter = RecordIter {
            reader: Reader {
                lines,
                widths,
                separator_length,
                flexible_width,
                header: None,
            },
        };

        let record1 = iter.next().unwrap().unwrap();
        assert_eq!(record1.line, "123456");
        assert_eq!(record1.ranges, vec![0..3, 3..6, 6..6]); // Last range should be empty

        let record2 = iter.next().unwrap().unwrap();
        assert_eq!(record2.line, "987654321");
        assert_eq!(record2.ranges, vec![0..3, 3..6, 6..9]);

        assert!(iter.next().is_none());
    }

    #[test]
    fn test_fwf_record_iter_with_separator() {
        let data = "123-456-789\n987-654-321\n".as_bytes();
        let lines = BufReader::new(Cursor::new(data)).lines();
        let widths = vec![3, 3, 3];
        let separator_length = 1;
        let flexible_width = false;

        let mut iter = RecordIter {
            reader: Reader {
                lines,
                widths,
                separator_length,
                flexible_width,
                header: None,
            },
        };

        let record1 = iter.next().unwrap().unwrap();
        assert_eq!(record1.line, "123-456-789");
        assert_eq!(record1.ranges, vec![0..3, 4..7, 8..11]);

        let record2 = iter.next().unwrap().unwrap();
        assert_eq!(record2.line, "987-654-321");
        assert_eq!(record2.ranges, vec![0..3, 4..7, 8..11]);

        assert!(iter.next().is_none());
    }

    #[test]
    fn test_fwf_record_iter_empty_line() {
        let data = "123456789\n\n987654321\n".as_bytes();
        let lines = BufReader::new(Cursor::new(data)).lines();
        let widths = vec![3, 3, 3];
        let separator_length = 0;
        let flexible_width = false;

        let mut iter = RecordIter {
            reader: Reader {
                lines,
                widths,
                separator_length,
                flexible_width,
                header: None,
            },
        };

        let record1 = iter.next().unwrap().unwrap();
        assert_eq!(record1.line, "123456789");
        assert_eq!(record1.ranges, vec![0..3, 3..6, 6..9]);

        let record2 = iter.next().unwrap();
        assert!(record2.is_err());
        assert!(matches!(record2.unwrap_err(), ReaderError::EmptyLine));

        let record3 = iter.next().unwrap().unwrap();
        assert_eq!(record3.line, "987654321");
        assert_eq!(record3.ranges, vec![0..3, 3..6, 6..9]);

        assert!(iter.next().is_none());
    }

    #[test]
    fn test_fwf_record_iter_width_mismatch() {
        let data = "12345\n987654321\n".as_bytes();
        let lines = BufReader::new(Cursor::new(data)).lines();
        let widths = vec![3, 3, 3];
        let separator_length = 0;
        let flexible_width = false;

        let mut iter = RecordIter {
            reader: Reader {
                lines,
                widths,
                separator_length,
                flexible_width,
                header: None,
            },
        };

        let record1 = iter.next().unwrap();
        assert!(record1.is_err());
        assert!(matches!(
            record1.unwrap_err(),
            ReaderError::WidthMismatch(3, 3)
        ));

        let record2 = iter.next().unwrap().unwrap();
        assert_eq!(record2.line, "987654321");
        assert_eq!(record2.ranges, vec![0..3, 3..6, 6..9]);

        assert!(iter.next().is_none());
    }

    #[test]
    fn test_fwf_record_iter_end_of_file() {
        let data = "123456789".as_bytes(); // No newline at the end
        let lines = BufReader::new(Cursor::new(data)).lines();
        let widths = vec![3, 3, 3];
        let separator_length = 0;
        let flexible_width = false;

        let mut iter = RecordIter {
            reader: Reader {
                lines,
                widths,
                separator_length,
                flexible_width,
                header: None,
            },
        };

        let record1 = iter.next().unwrap().unwrap();
        assert_eq!(record1.line, "123456789");
        assert_eq!(record1.ranges, vec![0..3, 3..6, 6..9]);

        assert!(iter.next().is_none());
    }

    #[test]
    fn test_create_fwf_record() {
        let line = "123456789".to_string();
        let widths = vec![3, 3, 3];
        let sep_len = 0;
        let flexible_widths = false;

        let record = Record::try_new(line.clone(), &widths, sep_len, flexible_widths);

        assert!(record.is_ok());
        let record = record.unwrap();
        assert_eq!(record.line, line);
        assert_eq!(record.ranges, vec![0..3, 3..6, 6..9]);
    }

    #[test]
    fn test_create_fwf_record_with_flexible_width() {
        let line = "1234567".to_string();
        let widths = vec![3, 3, 3, 3];
        let sep_len = 0;
        let flexible_widths = true;

        let record = Record::try_new(line.clone(), &widths, sep_len, flexible_widths);

        assert!(record.is_ok());
        let record = record.unwrap();
        assert_eq!(record.line, line);
        assert_eq!(record.ranges, vec![0..3, 3..6, 6..7, 7..7]);
    }

    #[test]
    fn test_create_fwf_record_with_mismatch_width() {
        let line = "12345".to_string();
        let widths = vec![3, 3, 3];
        let sep_len = 0;
        let flexible_widths = false;

        let record = Record::try_new(line, &widths, sep_len, flexible_widths);

        assert!(record.is_err());
        let err = record.unwrap_err();
        assert!(matches!(err, ReaderError::WidthMismatch(3, 3)));
    }

    #[test]
    fn test_create_fwf_record_with_separator() {
        let line = "123-456-789".to_string();
        let widths = vec![3, 3, 3];
        let sep_len = 1;
        let flexible_widths = false;

        let record = Record::try_new(line.clone(), &widths, sep_len, flexible_widths);

        assert!(record.is_ok());
        let record = record.unwrap();
        assert_eq!(record.line, line);
        assert_eq!(record.ranges, vec![0..3, 4..7, 8..11]);
    }

    #[test]
    fn test_get_field_by_index() {
        let line = "123456789".to_string();
        let widths = vec![3, 3, 3];
        let sep_len = 0;
        let flexible_widths = false;

        let record = Record::try_new(line.clone(), &widths, sep_len, flexible_widths).unwrap();

        assert_eq!(record.get(0), Some("123"));
        assert_eq!(record.get(1), Some("456"));
        assert_eq!(record.get(2), Some("789"));
        assert_eq!(record.get(3), None);
    }

    #[test]
    fn test_iterate_over_fields() {
        let line = "123456789".to_string();
        let widths = vec![3, 3, 3];
        let sep_len = 0;
        let flexible_widths = false;

        let record = Record::try_new(line.clone(), &widths, sep_len, flexible_widths).unwrap();
        let fields: Vec<&str> = record.iter().collect();

        assert_eq!(fields, vec!["123", "456", "789"]);
    }

    #[test]
    fn test_empty_line() {
        let line = "".to_string();
        let widths = vec![3, 3, 3];
        let sep_len = 0;
        let flexible_widths = false;

        let record = Record::try_new(line, &widths, sep_len, flexible_widths);

        assert!(record.is_err());
        let err = record.unwrap_err();
        assert!(matches!(err, ReaderError::EmptyLine));
    }
}
