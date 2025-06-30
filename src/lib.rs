use std::{
    collections::HashMap,
    fmt::Debug,
    io::{Read, Seek},
    path::{Path, PathBuf},
};

use cfb::CompoundFile;

use chrono::{DateTime, FixedOffset};
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

pub struct MsgReader<'c, 'p, F> {
    inner: &'c mut CompoundFile<F>,
    path: &'p Path,
}

pub struct Attachment {
    pub name: String,
    pub data: Vec<u8>,
}

impl Debug for Attachment {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Attachment")
            .field("name", &self.name)
            .field("data", &self.data.len())
            .finish()
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
    fn read_path_binary(&mut self, subpath: &Path) -> Result<Vec<u8>> {
        let mut content = self.inner.open_stream(self.path.join(subpath))?;
        let mut buf = vec![];
        content.read_to_end(&mut buf).unwrap();
        Ok(buf)
    }
    fn read_path_string(&mut self, subpath: &Path) -> Result<String> {
        let mut content = self.inner.open_stream(self.path.join(subpath))?;
        let mut buf = vec![];
        content.read_to_end(&mut buf).unwrap();
        String::from_utf16(&pack_u8s_to_u16s_le_padded(&buf))
            .map_err(|_e| MsgError::Encoding)
            .map(|x| x.trim_end_matches('\0').to_string())
    }
    pub fn subject(&mut self) -> Result<String> {
        self.read_simple_string("0037") // PR_SUBJECT
    }
    fn pr_sender_name(&mut self) -> Result<String> {
        self.read_simple_string("0C1A")
    }
    fn pr_sender_email_adress_str(&mut self) -> Result<String> {
        self.read_simple_string("0C19")
    }
    fn pr_smtp_sender_address(&mut self) -> Result<String> {
        self.read_simple_string("5D01")
    }
    fn pr_smtp_address(&mut self) -> Result<String> {
        self.read_simple_string("39FE")
    }
    fn sender_address(&mut self) -> Result<String> {
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
    fn pr_body_html(&mut self) -> Result<String> {
        let bin = self.read_simple_binary("1013")?;
        String::from_utf8(bin).map_err(|_| MsgError::Encoding)
    }
    fn pr_rtf_compressed(&mut self) -> Result<Vec<u8>> {
        self.read_simple_binary("1009")
    }
    fn rtf(&mut self) -> Result<String> {
        self.pr_rtf_compressed()
            .and_then(|comp_rtf| decompress_rtf(&comp_rtf).map_err(|_| MsgError::Encoding))
    }
    pub fn body(&mut self) -> Result<String> {
        self.pr_body_html().or_else(|_| self.rtf())
    }
    pub fn sent_date(&mut self) -> Result<DateTime<FixedOffset>> {
        let headers = self.pr_transport_message_headers()?;
        let dateline = headers
            .lines()
            .find(|x| x.starts_with("Date"))
            .ok_or(MsgError::Encoding)?
            .split_once(": ")
            .ok_or(MsgError::Encoding)?
            .1;
        chrono::DateTime::parse_from_rfc2822(dateline).map_err(|_| MsgError::Encoding)
    }
    fn recipients(&mut self) -> Result<HashMap<String, String>> {
        let recip_paths: Vec<_> = self
            .inner
            .read_storage(self.path)?
            .filter(|x| x.name().starts_with("__recip_version1.0_"))
            .map(|r| r.path().to_owned())
            .collect();
        recip_paths
            .iter()
            .map(|r| {
                let name = self.read_path_string(&r.join("__substg1.0_3001001F"))?;
                let address = self.read_path_string(&r.join("__substg1.0_39FE001F"))?;
                Ok((name, address))
            })
            .collect()
    }
    pub fn to(&mut self) -> Result<HashMap<String, String>> {
        let to_field = self.read_simple_string("0E04")?;
        let to_list: Vec<_> = to_field.split(";").map(|n| n.trim()).collect();
        let output: HashMap<String, String> = self
            .recipients()?
            .into_iter()
            .filter(|(k, _v)| to_list.contains(&&k[..]))
            .collect();
        Ok(output)
    }
    pub fn cc(&mut self) -> Result<HashMap<String, String>> {
        let cc_field = self.read_simple_string("0E03")?;
        let cc_list: Vec<_> = cc_field.split(";").map(|n| n.trim()).collect();
        let output: HashMap<String, String> = self
            .recipients()?
            .into_iter()
            .filter(|(k, _v)| cc_list.contains(&&k[..]))
            .collect();
        Ok(output)
    }
    pub fn bcc(&mut self) -> Result<HashMap<String, String>> {
        let bcc_field = self.read_simple_string("0E02")?;
        let bcc_list: Vec<_> = bcc_field.split(";").map(|n| n.trim()).collect();
        let output: HashMap<String, String> = self
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
                    .read_path_string(&a.join("__substg1.0_3704001F"))
                    .or_else(|_| self.read_path_string(&a.join("__substg1.0_3001001F")))?;
                let data = self.read_path_binary(&a.join("__substg1.0_37010102"))?;
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
