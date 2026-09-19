use std::borrow::Cow;
use std::collections::HashSet;

use quick_xml::XmlVersion;
use quick_xml::events::BytesStart;
use quick_xml::events::attributes::{AttrError, Attribute};
use quick_xml::name::{Namespace, NamespaceResolver, QName, ResolveResult};

use crate::error::XcStringsError;

use super::import_state::{CoreElement, ImportElement};

const XLIFF_1_2_NAMESPACE: &str = "urn:oasis:names:tc:xliff:document:1.2";

#[derive(Clone, Copy)]
enum NamespaceMode {
    OfficialQualified,
    LegacyUnqualified,
}

#[derive(Clone, Copy, Default)]
enum DocumentLifecycle {
    #[default]
    Before,
    Inside {
        depth: usize,
    },
    After,
}

#[derive(Default)]
pub(super) struct DocumentValidator {
    lifecycle: DocumentLifecycle,
    namespace_mode: Option<NamespaceMode>,
    root_name: Option<String>,
}

impl DocumentValidator {
    pub(super) fn new() -> Self {
        Self::default()
    }

    pub(super) fn start(
        &mut self,
        namespace: &ResolveResult<'_>,
        element: &BytesStart<'_>,
        resolver: &NamespaceResolver,
    ) -> Result<ImportElement, XcStringsError> {
        let local_name = element.local_name();
        validate_attributes(resolver, element, local_name.as_ref())?;

        match self.lifecycle {
            DocumentLifecycle::Before => {
                self.open_root(namespace, element, local_name.as_ref(), false)
            }
            DocumentLifecycle::Inside { depth } => {
                let child = self.validate_child(namespace, local_name.as_ref())?;
                self.lifecycle = DocumentLifecycle::Inside { depth: depth + 1 };
                Ok(child)
            }
            DocumentLifecycle::After => element_after_root(local_name.as_ref()),
        }
    }

    pub(super) fn empty(
        &mut self,
        namespace: &ResolveResult<'_>,
        element: &BytesStart<'_>,
        resolver: &NamespaceResolver,
    ) -> Result<ImportElement, XcStringsError> {
        let local_name = element.local_name();
        validate_attributes(resolver, element, local_name.as_ref())?;

        match self.lifecycle {
            DocumentLifecycle::Before => {
                self.open_root(namespace, element, local_name.as_ref(), true)
            }
            DocumentLifecycle::Inside { .. } => self.validate_child(namespace, local_name.as_ref()),
            DocumentLifecycle::After => element_after_root(local_name.as_ref()),
        }
    }

    pub(super) fn end(
        &mut self,
        namespace: &ResolveResult<'_>,
        local_name: &str,
    ) -> Result<ImportElement, XcStringsError> {
        let DocumentLifecycle::Inside { depth } = self.lifecycle else {
            return Err(XcStringsError::XliffParse(format!(
                "unexpected closing element </{}> outside <xliff> document root",
                local_name
            )));
        };

        let element = classify_element(self.namespace_mode, namespace, local_name)?;
        if depth == 1 {
            self.lifecycle = DocumentLifecycle::After;
        } else {
            self.lifecycle = DocumentLifecycle::Inside { depth: depth - 1 };
        }
        Ok(element)
    }

    pub(super) fn text(&self, text: &str) -> Result<(), XcStringsError> {
        if self.is_outside_root() && !is_xml_whitespace(text) {
            return Err(XcStringsError::XliffParse(
                "non-whitespace text is not allowed outside <xliff> document root".into(),
            ));
        }
        Ok(())
    }

    pub(super) fn cdata(&self) -> Result<(), XcStringsError> {
        if self.is_outside_root() {
            return Err(XcStringsError::XliffParse(
                "CDATA is not allowed outside <xliff> document root".into(),
            ));
        }
        Ok(())
    }

    pub(super) fn general_reference(&self) -> Result<(), XcStringsError> {
        if self.is_outside_root() {
            return Err(XcStringsError::XliffParse(
                "character references are not allowed outside <xliff> document root".into(),
            ));
        }
        Ok(())
    }

    pub(super) fn declaration(&self) -> Result<(), XcStringsError> {
        if !matches!(self.lifecycle, DocumentLifecycle::Before) {
            return Err(XcStringsError::XliffParse(
                "XML declaration is only allowed before <xliff> document root".into(),
            ));
        }
        Ok(())
    }

    pub(super) fn doctype(&self) -> Result<(), XcStringsError> {
        if !matches!(self.lifecycle, DocumentLifecycle::Before) {
            return Err(XcStringsError::XliffParse(
                "DOCTYPE is only allowed before <xliff> document root".into(),
            ));
        }
        Ok(())
    }

    pub(super) fn finish(&self) -> Result<(), XcStringsError> {
        match self.lifecycle {
            DocumentLifecycle::Before => Err(XcStringsError::XliffParse(
                "missing <xliff> document root".into(),
            )),
            DocumentLifecycle::Inside { .. } => {
                let root_name = self.root_name.as_deref().unwrap_or("xliff");
                Err(XcStringsError::XliffParse(format!(
                    "start tag not closed: `</{}>` not found before end of input",
                    root_name
                )))
            }
            DocumentLifecycle::After => Ok(()),
        }
    }

    fn open_root(
        &mut self,
        namespace: &ResolveResult<'_>,
        element: &BytesStart<'_>,
        local_name: &str,
        empty: bool,
    ) -> Result<ImportElement, XcStringsError> {
        if local_name != "xliff" {
            return Err(XcStringsError::XliffParse(format!(
                "document root must be <xliff>; found <{}>",
                local_name
            )));
        }

        self.namespace_mode = Some(namespace_mode_from_root(namespace, local_name)?);
        self.root_name = Some(element.name().as_ref().to_owned());
        self.lifecycle = if empty {
            DocumentLifecycle::After
        } else {
            DocumentLifecycle::Inside { depth: 1 }
        };
        Ok(ImportElement::core(CoreElement::Xliff, local_name))
    }

    fn validate_child(
        &self,
        namespace: &ResolveResult<'_>,
        local_name: &str,
    ) -> Result<ImportElement, XcStringsError> {
        if local_name == "xliff" {
            return Err(XcStringsError::XliffParse(
                "nested <xliff> element is not allowed".into(),
            ));
        }
        classify_element(self.namespace_mode, namespace, local_name)
    }

    fn is_outside_root(&self) -> bool {
        !matches!(self.lifecycle, DocumentLifecycle::Inside { .. })
    }
}

fn element_after_root<T>(local_name: &str) -> Result<T, XcStringsError> {
    Err(XcStringsError::XliffParse(format!(
        "element <{}> appears after </xliff> document root",
        local_name
    )))
}

fn is_xml_whitespace(text: &str) -> bool {
    text.bytes()
        .all(|byte| matches!(byte, b' ' | b'\t' | b'\n' | b'\r'))
}

fn validate_attributes(
    resolver: &NamespaceResolver,
    element: &BytesStart<'_>,
    local_name: &str,
) -> Result<(), XcStringsError> {
    let mut expanded_names = HashSet::new();
    for attribute in element.attributes() {
        let attribute = match attribute {
            Ok(attribute) => attribute,
            Err(AttrError::Duplicated(_, _)) => {
                return Err(XcStringsError::XliffParse(format!(
                    "duplicate attribute on <{}>",
                    local_name
                )));
            }
            Err(error) => return Err(XcStringsError::XliffParse(error.to_string())),
        };

        if is_namespace_declaration(attribute.key.as_ref()) {
            normalize_namespace_value(attribute.value.as_ref(), local_name)?;
            continue;
        }

        let (namespace, attribute_local_name) = resolver.resolve_attribute(attribute.key);
        let namespace = match namespace {
            ResolveResult::Unbound => None,
            ResolveResult::Bound(namespace) => Some(normalize_namespace(namespace, local_name)?),
            ResolveResult::Unknown(prefix) => {
                return Err(XcStringsError::XliffParse(format!(
                    "attribute <{}> on <{}> uses unbound namespace prefix '{}'",
                    attribute.key.as_ref(),
                    local_name,
                    prefix
                )));
            }
        };
        let attribute_local_name = attribute_local_name.as_ref().to_owned();

        if !expanded_names.insert((namespace.clone(), attribute_local_name.clone())) {
            let expanded_name = match namespace {
                Some(namespace) => format!("{{{namespace}}}{attribute_local_name}"),
                None => attribute_local_name,
            };
            return Err(XcStringsError::XliffParse(format!(
                "duplicate expanded attribute '{expanded_name}' on <{}>",
                local_name
            )));
        }
    }
    Ok(())
}

fn is_namespace_declaration(name: &str) -> bool {
    name == "xmlns" || name.starts_with("xmlns:")
}

fn namespace_mode_from_root(
    namespace: &ResolveResult<'_>,
    local_name: &str,
) -> Result<NamespaceMode, XcStringsError> {
    let namespace = canonical_element_namespace(namespace, local_name)?;
    match namespace.as_deref() {
        None => Ok(NamespaceMode::LegacyUnqualified),
        Some(XLIFF_1_2_NAMESPACE) => Ok(NamespaceMode::OfficialQualified),
        _ => namespace_error(namespace.as_deref(), local_name),
    }
}

fn classify_element(
    namespace_mode: Option<NamespaceMode>,
    namespace: &ResolveResult<'_>,
    local_name: &str,
) -> Result<ImportElement, XcStringsError> {
    let namespace = canonical_element_namespace(namespace, local_name)?;
    let Some(namespace_mode) = namespace_mode else {
        return Err(XcStringsError::XliffParse(format!(
            "element <{}> has no document namespace mode",
            local_name
        )));
    };

    let core = CoreElement::from_local_name(local_name);
    match (namespace_mode, namespace.as_deref(), core) {
        (NamespaceMode::OfficialQualified, Some(XLIFF_1_2_NAMESPACE), Some(core))
        | (NamespaceMode::LegacyUnqualified, None, Some(core)) => {
            Ok(ImportElement::core(core, local_name))
        }
        (NamespaceMode::OfficialQualified, Some(XLIFF_1_2_NAMESPACE), None) => {
            Ok(ImportElement::core(CoreElement::Other, local_name))
        }
        (NamespaceMode::OfficialQualified, None, Some(_)) => {
            Err(XcStringsError::XliffParse(format!(
                "element <{}> is unqualified in namespace-qualified XLIFF document; expected '{}'",
                local_name, XLIFF_1_2_NAMESPACE
            )))
        }
        (NamespaceMode::LegacyUnqualified, Some(XLIFF_1_2_NAMESPACE), Some(_)) => {
            Err(XcStringsError::XliffParse(format!(
                "element <{}> uses namespace '{}' in legacy unqualified XLIFF document; expected no namespace",
                local_name, XLIFF_1_2_NAMESPACE
            )))
        }
        (_, Some(namespace), Some(_)) => namespace_error(Some(namespace), local_name),
        _ => Ok(ImportElement::extension(local_name)),
    }
}

fn canonical_element_namespace(
    namespace: &ResolveResult<'_>,
    local_name: &str,
) -> Result<Option<String>, XcStringsError> {
    match namespace {
        ResolveResult::Bound(namespace) => normalize_namespace(*namespace, local_name).map(Some),
        ResolveResult::Unknown(prefix) => Err(XcStringsError::XliffParse(format!(
            "element <{}> uses unbound namespace prefix '{}'",
            local_name, prefix
        ))),
        ResolveResult::Unbound => Ok(None),
    }
}

fn normalize_namespace(
    namespace: Namespace<'_>,
    local_name: &str,
) -> Result<String, XcStringsError> {
    normalize_namespace_value(namespace.as_ref(), local_name)
}

fn normalize_namespace_value(value: &str, local_name: &str) -> Result<String, XcStringsError> {
    Attribute {
        key: QName("xmlns"),
        value: Cow::Borrowed(value),
    }
    .normalized_value(XmlVersion::Implicit1_0)
    .map(|value| value.into_owned())
    .map_err(|_| {
        XcStringsError::XliffParse(format!("invalid XML namespace value on <{}>", local_name))
    })
}

fn namespace_error<T>(namespace: Option<&str>, local_name: &str) -> Result<T, XcStringsError> {
    match namespace {
        Some(namespace) => Err(XcStringsError::XliffParse(format!(
            "element <{}> uses namespace '{}'; expected '{}'",
            local_name, namespace, XLIFF_1_2_NAMESPACE
        ))),
        None => Err(XcStringsError::XliffParse(format!(
            "element <{}> has no document namespace mode",
            local_name
        ))),
    }
}
