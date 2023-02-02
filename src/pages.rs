use std::{ops::Range, path::PathBuf};

use crate::{dict::Paths, resource::Rsc, Error, PageItemId};

const RSC_NAME: &str = "contents";

pub struct Pages {
    path: PathBuf,
    res: Option<Rsc>,
}

pub struct XmlParser<'a> {
    xml: &'a str,
    tokens: xmlparser::Tokenizer<'a>,
    target_level: Option<usize>,
    tag_stack: Vec<(&'a str, usize)>,
}

impl<'a> XmlParser<'a> {
    pub fn from(xml: &'a str) -> Self {
        Self {
            xml,
            tokens: xmlparser::Tokenizer::from(xml),
            target_level: None,
            tag_stack: Vec::new(),
        }
    }

    pub fn next_fragment_by(
        &mut self,
        elem_cond: impl Fn(&str) -> bool,
        attr_cond: impl Fn(&str, &str) -> bool,
    ) -> Result<Option<&'a str>, Error> {
        use xmlparser::{
            ElementEnd::{Close, Empty},
            Token::{Attribute, ElementEnd, ElementStart},
        };

        for token in &mut self.tokens {
            let mut popped = None;
            let token = token?;
            match token {
                ElementStart { local, span, .. } => {
                    self.tag_stack.push((local.as_str(), span.start()));
                    if elem_cond(&local) && self.target_level.is_none() {
                        self.target_level = Some(self.tag_stack.len());
                    }
                }
                Attribute { local, value, .. } => {
                    if attr_cond(&local, &value) && self.target_level.is_none() {
                        self.target_level = Some(self.tag_stack.len());
                    }
                }
                ElementEnd {
                    end: Close(_, tag),
                    span,
                } => {
                    if Some(&*tag) == self.tag_stack.last().map(|(t, _)| *t) {
                        popped = self.tag_stack.pop().map(|(_, start)| (start, span.end()));
                    } else {
                        return Err(Error::XmlError);
                    }
                }
                ElementEnd { end: Empty, span } => {
                    popped = self.tag_stack.pop().map(|(_, start)| (start, span.end()));
                }
                _ => continue,
            }
            if let Some((start, end)) = popped {
                if Some(self.tag_stack.len()) < self.target_level {
                    self.target_level = None;
                    return Ok(Some(&self.xml[start..end]));
                }
            }
        }
        // No body fragment or item fragment with suitable ID found
        Ok(None)
    }
}

impl Pages {
    pub fn new(paths: &Paths) -> Result<Self, Error> {
        Ok(Pages {
            path: paths.contents_path().join(RSC_NAME),
            res: None,
        })
    }

    pub fn init(&mut self) -> Result<(), Error> {
        if self.res.is_none() {
            self.res = Some(Rsc::new(&self.path, RSC_NAME)?);
        }
        Ok(())
    }

    pub fn get_page(&mut self, id: PageItemId) -> Result<&str, Error> {
        self.init()?;
        let Some(res) = self.res.as_mut() else { unreachable!() };
        let xml = std::str::from_utf8(res.get(id.page)?).map_err(|_| Error::Utf8Error)?;
        Ok(xml)
    }

    pub fn get_item(&mut self, id: PageItemId) -> Result<&str, Error> {
        let xml = self.get_page(id)?;
        let mut parser = XmlParser::from(xml);
        if id.item == 0 {
            parser.next_fragment_by(|tag| tag == "body", |_, _| false)
        } else {
            parser.next_fragment_by(
                |_| false,
                |name, value| {
                    if name == "id" {
                        if let Some((page, item)) = value.split_once('-') {
                            if page.parse() == Ok(id.page) && item.parse() == Ok(id.item) {
                                return true;
                            }
                        }
                    }
                    false
                },
            )
        }?
        .ok_or(Error::XmlError)
    }

    pub fn get_item_audio(&mut self, id: PageItemId) -> Result<AudioIter, Error> {
        let xml = self.get_item(id)?;
        let parser = XmlParser::from(xml);
        Ok(AudioIter { parser })
    }

    pub fn page_by_idx(&mut self, idx: usize) -> Result<(u32, &str), Error> {
        self.init()?;
        let Some(res) = self.res.as_mut() else { unreachable!() };
        let (id, page) = res.get_by_idx(idx)?;
        Ok((id, std::str::from_utf8(page).map_err(|_| Error::Utf8Error)?))
    }

    pub fn idx_iter(&mut self) -> Result<Range<usize>, Error> {
        self.init()?;
        let Some(res) = self.res.as_ref() else { unreachable!() };
        Ok(0..res.len())
    }
}

pub struct AudioIter<'a> {
    parser: XmlParser<'a>,
}

impl<'a> Iterator for AudioIter<'a> {
    type Item = Result<&'a str, Error>;

    fn next(&mut self) -> Option<Self::Item> {
        self.parser
            .next_fragment_by(
                |_| false,
                |name, value| name == "href" && value.ends_with(".aac"),
            )
            .transpose()
    }
}
