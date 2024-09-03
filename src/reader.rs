use std::{
    fs::File,
    io::{BufRead, BufReader, Lines},
    ops::Range,
    path::PathBuf,
};

use crate::ReaderError;

pub struct FwfFileReader {
    file: PathBuf,
    widths: Vec<usize>,
    separator_length: usize,
    flexible_width: bool,
    has_header: bool,
}

impl FwfFileReader {
    pub fn new(file: PathBuf, widths: Vec<usize>) -> Self {
        Self {
            file,
            widths,
            separator_length: 1,
            flexible_width: true,
            has_header: true,
        }
    }

    pub fn with_separator_length(&mut self, separator_length: usize) -> &mut Self {
        self.separator_length = separator_length;
        self
    }

    pub fn with_flexible_width(&mut self, flexible_width: bool) -> &mut Self {
        self.flexible_width = flexible_width;
        self
    }

    pub fn with_has_header(&mut self, has_header: bool) -> &mut Self {
        self.has_header = has_header;
        self
    }

    pub fn header(&self) -> Result<Option<FwfRecord>, ReaderError> {
        Ok(if self.has_header {
            let mut reader = BufReader::new(File::open(&self.file)?).lines();
            let line = reader.next().ok_or(ReaderError::EmptyLine)??;
            Some(FwfRecord::try_new(
                line,
                &self.widths,
                self.separator_length,
                self.flexible_width,
            )?)
        } else {
            None
        })
    }

    pub fn records(&self) -> Result<FwfRecordIter<BufReader<File>>, ReaderError> {
        let mut reader = BufReader::new(File::open(&self.file)?).lines();
        if self.has_header {
            reader.next();
        }
        Ok(FwfRecordIter {
            reader,
            widths: self.widths.clone(),
            separator_length: self.separator_length,
            flexible_width: self.flexible_width,
        })
    }
}

#[derive(Debug)]
pub struct FwfRecordIter<R> {
    reader: Lines<R>,
    widths: Vec<usize>,
    separator_length: usize,
    flexible_width: bool,
}

impl<R> Iterator for FwfRecordIter<R>
where
    R: BufRead,
{
    type Item = Result<FwfRecord, ReaderError>;

    fn next(&mut self) -> Option<Self::Item> {
        self.reader.next().map(|result| {
            FwfRecord::try_new(
                result?,
                &self.widths,
                self.separator_length,
                self.flexible_width,
            )
        })
    }
}

#[derive(Debug, Clone)]
pub struct FwfRecord {
    line: String,
    ranges: Vec<Range<usize>>,
}

impl FwfRecord {
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
    fwr: &'a FwfRecord,
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
    use std::io::Cursor;
    use std::io::Write;

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

        let mut reader = FwfFileReader::new(file_path.clone(), widths);
        reader
            .with_separator_length(0)
            .with_flexible_width(false)
            .with_has_header(true);

        let header = reader.header().unwrap().unwrap();
        assert_eq!(header.get(0), Some("header1"));
        assert_eq!(header.get(1), Some("header2"));
        assert_eq!(header.get(2), Some("header3"));

        let mut records = reader.records().unwrap();
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

        let mut reader = FwfFileReader::new(file_path.clone(), widths);
        reader
            .with_separator_length(0)
            .with_flexible_width(false)
            .with_has_header(false);

        let header = reader.header().unwrap();
        assert!(header.is_none());

        let mut records = reader.records().unwrap();
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

        let mut reader = FwfFileReader::new(file_path.clone(), widths);
        reader
            .with_separator_length(1)
            .with_flexible_width(false)
            .with_has_header(false);

        let mut records = reader.records().unwrap();
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

        let mut reader = FwfFileReader::new(file_path.clone(), widths);
        reader
            .with_separator_length(0)
            .with_flexible_width(true)
            .with_has_header(false);

        let mut records = reader.records().unwrap();
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

        let mut reader = FwfFileReader::new(file_path.clone(), widths);
        reader
            .with_separator_length(0)
            .with_flexible_width(false)
            .with_has_header(false);

        let header = reader.header().unwrap();
        assert!(header.is_none());

        let records = reader.records().unwrap();
        assert!(records.into_iter().next().is_none());

        delete_test_file(&file_path).unwrap();
    }

    #[test]
    fn test_fwf_file_reader_nonexistent_file() {
        let file_path = PathBuf::from("nonexistent_file.txt");
        let widths = vec![3, 3, 3];

        let reader = FwfFileReader::new(file_path, widths);

        let header_result = reader.header();
        assert!(header_result.is_err());

        let records_result = reader.records();
        assert!(records_result.is_err());
    }

    #[test]
    fn test_fwf_record_iter_basic() {
        let data = "123456789\n987654321\n".as_bytes();
        let reader = Cursor::new(data).lines();
        let widths = vec![3, 3, 3];
        let separator_length = 0;
        let flexible_width = false;

        let mut iter = FwfRecordIter {
            reader,
            widths: widths.clone(),
            separator_length,
            flexible_width,
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
        let reader = Cursor::new(data).lines();
        let widths = vec![3, 3, 3];
        let separator_length = 0;
        let flexible_width = true;

        let mut iter = FwfRecordIter {
            reader,
            widths: widths.clone(),
            separator_length,
            flexible_width,
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
        let reader = Cursor::new(data).lines();
        let widths = vec![3, 3, 3];
        let separator_length = 1;
        let flexible_width = false;

        let mut iter = FwfRecordIter {
            reader,
            widths: widths.clone(),
            separator_length,
            flexible_width,
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
        let reader = Cursor::new(data).lines();
        let widths = vec![3, 3, 3];
        let separator_length = 0;
        let flexible_width = false;

        let mut iter = FwfRecordIter {
            reader,
            widths: widths.clone(),
            separator_length,
            flexible_width,
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
        let reader = Cursor::new(data).lines();
        let widths = vec![3, 3, 3];
        let separator_length = 0;
        let flexible_width = false;

        let mut iter = FwfRecordIter {
            reader,
            widths: widths.clone(),
            separator_length,
            flexible_width,
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
        let reader = Cursor::new(data).lines();
        let widths = vec![3, 3, 3];
        let separator_length = 0;
        let flexible_width = false;

        let mut iter = FwfRecordIter {
            reader,
            widths: widths.clone(),
            separator_length,
            flexible_width,
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

        let record = FwfRecord::try_new(line.clone(), &widths, sep_len, flexible_widths);

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

        let record = FwfRecord::try_new(line.clone(), &widths, sep_len, flexible_widths);

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

        let record = FwfRecord::try_new(line, &widths, sep_len, flexible_widths);

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

        let record = FwfRecord::try_new(line.clone(), &widths, sep_len, flexible_widths);

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

        let record = FwfRecord::try_new(line.clone(), &widths, sep_len, flexible_widths).unwrap();

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

        let record = FwfRecord::try_new(line.clone(), &widths, sep_len, flexible_widths).unwrap();
        let fields: Vec<&str> = record.iter().collect();

        assert_eq!(fields, vec!["123", "456", "789"]);
    }

    #[test]
    fn test_empty_line() {
        let line = "".to_string();
        let widths = vec![3, 3, 3];
        let sep_len = 0;
        let flexible_widths = false;

        let record = FwfRecord::try_new(line, &widths, sep_len, flexible_widths);

        assert!(record.is_err());
        let err = record.unwrap_err();
        assert!(matches!(err, ReaderError::EmptyLine));
    }
}
