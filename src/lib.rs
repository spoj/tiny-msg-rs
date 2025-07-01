use std::{
    fmt::Debug,
    io::{Cursor, Read, Seek},
    path::{Path, PathBuf},
};

use cfb::CompoundFile;

use chrono::{DateTime, Utc};
use compressed_rtf::decompress_rtf;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum MsgError {
    #[error("IO error: {0}")]
    IO(#[from] std::io::Error),
    #[error("Format error: {0}")]
    Fmt(#[from] std::fmt::Error),
    #[error("Encoding error")]
    Encoding,
    #[error("Unknown error")]
    Unknown,
}

type Result<S> = std::result::Result<S, MsgError>;

/// A low-level API for reading data from a .msg file.
pub struct MsgReader<'c, 'p, F> {
    inner: &'c mut CompoundFile<F>,
    path: &'p Path,
}

#[derive(Clone)]
pub struct Attachment {
    pub name: String,
    pub data: Vec<u8>,
}

impl Debug for Attachment {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Attachment")
            .field("name", &self.name)
            .field("data of size", &self.data.len())
            .finish()
    }
}

/// A high-level API for interacting with .msg files, providing an owned data structure.
#[derive(Debug, Clone)]
pub struct Email {
    pub from: Option<(String, String)>,
    pub sent_date: Option<chrono::DateTime<Utc>>,
    pub to: Vec<(String, String)>,
    pub cc: Vec<(String, String)>,
    pub bcc: Vec<(String, String)>,
    pub subject: Option<String>,
    pub body: Option<String>,
    pub attachments: Vec<Attachment>,
    pub embedded_messages: Vec<Email>,
}

impl Email {
    pub fn from_path<P: AsRef<Path>>(file: P) -> Self {
        Self::from_path_internal(file.as_ref(), Path::new("/"))
    }
    pub fn from_bytes<B: AsRef<[u8]>>(bytes: B) -> Self {
        Self::from_bytes_internal(bytes.as_ref(), Path::new("/"))
    }

    fn from_path_internal(file: &Path, subpath: &Path) -> Self {
        let mut comp = cfb::open(file).unwrap();
        let mut reader = MsgReader::new(&mut comp, subpath);
        let from = reader.from().ok();
        let sent_date = reader.sent_date().ok();
        let to = reader.to().unwrap_or_default();
        let cc = reader.cc().unwrap_or_default();
        let bcc = reader.bcc().unwrap_or_default();
        let subject = reader.pr_subject().ok();
        let body = reader.body().ok();
        let attachments = reader.attachments().unwrap_or_default();
        let emb_paths = reader.embedded_messages().unwrap();
        let embedded_messages: Vec<_> = emb_paths
            .into_iter()
            .map(|emb_path| Self::from_path_internal(file, &emb_path))
            .collect();
        Self {
            from,
            sent_date,
            to,
            cc,
            bcc,
            subject,
            body,
            attachments,
            embedded_messages,
        }
    }
    fn from_bytes_internal(bytes: &[u8], subpath: &Path) -> Self {
        let cur = Cursor::new(bytes);
        let mut comp = CompoundFile::open(cur).unwrap();
        let mut reader = MsgReader::new(&mut comp, subpath);
        let from = reader.from().ok();
        let sent_date = reader.sent_date().ok();
        let to = reader.to().unwrap_or_default();
        let cc = reader.cc().unwrap_or_default();
        let bcc = reader.bcc().unwrap_or_default();
        let subject = reader.pr_subject().ok();
        let body = reader.body().ok();
        let attachments = reader.attachments().unwrap_or_default();
        let emb_paths = reader.embedded_messages().unwrap();
        let embedded_messages: Vec<_> = emb_paths
            .into_iter()
            .map(|emb_path| Self::from_bytes_internal(bytes, &emb_path))
            .collect();
        Self {
            from,
            sent_date,
            to,
            cc,
            bcc,
            subject,
            body,
            attachments,
            embedded_messages,
        }
    }
}

impl<'c, 'p, F> MsgReader<'c, 'p, F>
where
    F: Read + Seek,
{
    pub fn new(inner: &'c mut CompoundFile<F>, path: &'p Path) -> Self {
        Self { inner, path }
    }

    fn read_simple_string(&mut self, prop: &str) -> Result<String> {
        let mut content = self
            .inner
            .open_stream(self.path.join(format!("__substg1.0_{prop}001F")))?;
        let mut buf = vec![];
        content.read_to_end(&mut buf).unwrap();
        String::from_utf16(&pack_u8s_to_u16s_le_padded(&buf))
            .map_err(|_e| MsgError::Encoding)
            .map(|x| x.trim_end_matches('\0').to_string())
    }
    fn read_simple_binary(&mut self, prop: &str) -> Result<Vec<u8>> {
        let mut content = self
            .inner
            .open_stream(self.path.join(format!("__substg1.0_{prop}0102")))?;
        let mut buf = vec![];
        content.read_to_end(&mut buf).unwrap();
        Ok(buf)
    }
    pub fn read_path_as_binary(&mut self, subpath: &Path) -> Result<Vec<u8>> {
        let mut content = self.inner.open_stream(self.path.join(subpath))?;
        let mut buf = vec![];
        content.read_to_end(&mut buf).unwrap();
        Ok(buf)
    }
    pub fn read_path_as_string(&mut self, subpath: &Path) -> Result<String> {
        let mut content = self.inner.open_stream(self.path.join(subpath))?;
        let mut buf = vec![];
        content.read_to_end(&mut buf).unwrap();
        String::from_utf16(&pack_u8s_to_u16s_le_padded(&buf))
            .map_err(|_e| MsgError::Encoding)
            .map(|x| x.trim_end_matches('\0').to_string())
    }
    pub fn pr_subject(&mut self) -> Result<String> {
        self.read_simple_string("0037") // PR_SUBJECT
    }
    pub fn pr_sender_name(&mut self) -> Result<String> {
        self.read_simple_string("0C1A")
    }
    pub fn pr_sender_email_adress_str(&mut self) -> Result<String> {
        self.read_simple_string("0C19")
    }
    pub fn pr_smtp_sender_address(&mut self) -> Result<String> {
        self.read_simple_string("5D01")
    }
    pub fn pr_smtp_address(&mut self) -> Result<String> {
        self.read_simple_string("39FE")
    }
    pub fn sender_address(&mut self) -> Result<String> {
        self.pr_sender_email_adress_str()
            .or_else(|_| self.pr_smtp_address())
            .or_else(|_| self.pr_smtp_sender_address())
    }
    pub fn from(&mut self) -> Result<(String, String)> {
        Ok((self.pr_sender_name()?, self.sender_address()?))
    }
    pub fn pr_transport_message_headers(&mut self) -> Result<String> {
        self.read_simple_string("007D")
    }
    pub fn pr_body_html(&mut self) -> Result<String> {
        let bin = self.read_simple_binary("1013")?;
        String::from_utf8(bin).map_err(|_| MsgError::Encoding)
    }
    pub fn pr_rtf_compressed(&mut self) -> Result<Vec<u8>> {
        self.read_simple_binary("1009")
    }
    fn rtf(&mut self) -> Result<String> {
        self.pr_rtf_compressed()
            .and_then(|comp_rtf| decompress_rtf(&comp_rtf).map_err(|_| MsgError::Encoding))
    }
    pub fn body(&mut self) -> Result<String> {
        self.pr_body_html().or_else(|_| self.rtf())
    }
    pub fn sent_date(&mut self) -> Result<DateTime<Utc>> {
        let headers = self.pr_transport_message_headers()?;
        let dateline = headers
            .lines()
            .find(|x| x.starts_with("Date"))
            .ok_or(MsgError::Encoding)?
            .split_once(": ")
            .ok_or(MsgError::Encoding)?
            .1;
        chrono::DateTime::parse_from_rfc2822(dateline)
            .map_err(|_| MsgError::Encoding)
            .map(|d| d.with_timezone(&Utc))
    }
    fn recipients(&mut self) -> Result<Vec<(String, String)>> {
        let recip_paths: Vec<_> = self
            .inner
            .read_storage(self.path)?
            .filter(|x| x.name().starts_with("__recip_version1.0_"))
            .map(|r| r.path().to_owned())
            .collect();
        recip_paths
            .iter()
            .map(|r| {
                let name = self.read_path_as_string(&r.join("__substg1.0_3001001F"))?;
                let address = self.read_path_as_string(&r.join("__substg1.0_39FE001F"))?;
                Ok((name, address))
            })
            .collect()
    }
    pub fn to(&mut self) -> Result<Vec<(String, String)>> {
        let to_field = self.read_simple_string("0E04")?;
        let to_list: Vec<_> = to_field.split(";").map(|n| n.trim()).collect();
        let output: Vec<(String, String)> = self
            .recipients()?
            .into_iter()
            .filter(|(k, _v)| to_list.contains(&&k[..]))
            .collect();
        Ok(output)
    }
    pub fn cc(&mut self) -> Result<Vec<(String, String)>> {
        let cc_field = self.read_simple_string("0E03")?;
        let cc_list: Vec<_> = cc_field.split(";").map(|n| n.trim()).collect();
        let output: Vec<(String, String)> = self
            .recipients()?
            .into_iter()
            .filter(|(k, _v)| cc_list.contains(&&k[..]))
            .collect();
        Ok(output)
    }
    pub fn bcc(&mut self) -> Result<Vec<(String, String)>> {
        let bcc_field = self.read_simple_string("0E02")?;
        let bcc_list: Vec<_> = bcc_field.split(";").map(|n| n.trim()).collect();
        let output: Vec<(String, String)> = self
            .recipients()?
            .into_iter()
            .filter(|(k, _v)| bcc_list.contains(&&k[..]))
            .collect();
        Ok(output)
    }
    pub fn attachments(&mut self) -> Result<Vec<Attachment>> {
        let attachment_paths: Vec<_> = self
            .inner
            .read_storage(self.path)?
            .filter(|x| x.name().starts_with("__attach_version1.0_"))
            .map(|r| r.path().to_owned())
            .collect();
        let res = attachment_paths
            .iter()
            .flat_map(|a| {
                let name = self
                    .read_path_as_string(&a.join("__substg1.0_3704001F"))
                    .or_else(|_| self.read_path_as_string(&a.join("__substg1.0_3001001F")))?;
                let data = self.read_path_as_binary(&a.join("__substg1.0_37010102"))?;
                let output: Result<Attachment> = Ok(Attachment { name, data });
                output
            })
            .collect();
        Ok(res)
    }
    pub fn embedded_messages(&mut self) -> Result<Vec<PathBuf>> {
        let attachment_paths: Vec<_> = self
            .inner
            .read_storage(self.path)?
            .filter(|x| x.name().starts_with("__attach_version1.0_"))
            .map(|r| r.path().to_owned())
            .collect();
        let res = attachment_paths
            .into_iter()
            .map(|a| a.join("__substg1.0_3701000D"))
            .filter(|a| self.inner.is_storage(a))
            .collect();
        Ok(res)
    }
}

fn pack_u8s_to_u16s_le_padded(bytes: &[u8]) -> Vec<u16> {
    let mut result = Vec::with_capacity(bytes.len().div_ceil(2));
    let mut i = 0;
    while i < bytes.len() {
        let lsb = bytes[i];
        let msb = if i + 1 < bytes.len() {
            bytes[i + 1]
        } else {
            // Pad with zero if there's an odd number of bytes
            0x00
        };
        result.push(u16::from_le_bytes([lsb, msb]));
        i += 2; // Move to the next pair
    }
    result
}
