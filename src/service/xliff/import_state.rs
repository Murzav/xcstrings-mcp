use std::collections::HashSet;

use crate::error::XcStringsError;
use crate::model::translation::CompletedTranslation;

mod extensions;
mod grammar;
mod helpers;
mod ignored_core_grammar;
mod unit_ids;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum CoreElement {
    Xliff,
    File,
    Header,
    Body,
    Group,
    TransUnit,
    Source,
    Target,
    SegSource,
    AltTrans,
    BinUnit,
    Metadata,
    Inline,
    Other,
}

impl CoreElement {
    pub(super) fn from_local_name(name: &[u8]) -> Option<Self> {
        match name {
            b"xliff" => Some(Self::Xliff),
            b"file" => Some(Self::File),
            b"header" => Some(Self::Header),
            b"body" => Some(Self::Body),
            b"group" => Some(Self::Group),
            b"trans-unit" => Some(Self::TransUnit),
            b"source" => Some(Self::Source),
            b"target" => Some(Self::Target),
            b"seg-source" => Some(Self::SegSource),
            b"alt-trans" => Some(Self::AltTrans),
            b"bin-unit" => Some(Self::BinUnit),
            b"context-group" | b"count-group" | b"note" => Some(Self::Metadata),
            b"g" | b"x" | b"bx" | b"ex" | b"bpt" | b"ept" | b"sub" | b"it" | b"ph" | b"mrk" => {
                Some(Self::Inline)
            }
            b"skl" | b"external-file" | b"internal-file" | b"glossary" | b"reference"
            | b"phase-group" | b"phase" | b"tool" | b"context" | b"count" | b"prop-group"
            | b"prop" | b"bin-source" | b"bin-target" => Some(Self::Other),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ImportElement {
    pub(super) kind: ImportElementKind,
    pub(super) name: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum ImportElementKind {
    Core(CoreElement),
    Extension,
}

impl ImportElement {
    pub(super) fn core(kind: CoreElement, name: &[u8]) -> Self {
        Self {
            kind: ImportElementKind::Core(kind),
            name: String::from_utf8_lossy(name).into_owned(),
        }
    }

    pub(super) fn extension(name: &[u8]) -> Self {
        Self {
            kind: ImportElementKind::Extension,
            name: String::from_utf8_lossy(name).into_owned(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FilePhase {
    Start,
    HeaderSeen,
    BodySeen,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum UnitPhase {
    NeedSource,
    AfterSource,
    AfterSegSource,
    AfterTarget,
    Tail,
    Extensions,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BinUnitPhase {
    NeedSource,
    AfterSource,
    AfterTarget,
    Tail,
    Extensions,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum HeaderPhase {
    Start,
    Skeleton,
    PhaseGroup,
    Metadata,
    Extensions,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum GroupPhase {
    Start,
    Context,
    Count,
    Property,
    Note,
    Extensions,
    Structural,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AltPhase {
    Start,
    Source,
    SegSource,
    Target,
    Context,
    Property,
    Note,
    Extensions,
}

struct UnitData {
    id: String,
    locale: String,
    phase: UnitPhase,
    target: Option<String>,
}

enum FrameData {
    Root {
        file_count: usize,
        extension_needs_file: bool,
    },
    File {
        locale: String,
        phase: FilePhase,
        unit_ids: HashSet<String>,
    },
    Group {
        phase: GroupPhase,
        structural_count: usize,
    },
    Unit(UnitData),
    Source,
    Target {
        main_unit: bool,
    },
    Header {
        phase: HeaderPhase,
    },
    Body,
    AltTrans {
        phase: AltPhase,
    },
    BinUnit {
        phase: BinUnitPhase,
    },
    ChildList {
        expected: &'static str,
        minimum: usize,
        count: usize,
    },
    BinaryContainer {
        child_seen: bool,
    },
    Leaf,
    AnyContent,
    Inline,
    Opaque,
    Extension,
}

struct Frame {
    element: ImportElement,
    data: FrameData,
}

#[derive(Default)]
pub(super) struct ImportState {
    stack: Vec<Frame>,
    target_locale: Option<String>,
    translations: Vec<CompletedTranslation>,
    document_unit_ids: HashSet<String>,
    root_file_count: Option<usize>,
    root_extension_needs_file: bool,
}

impl ImportState {
    pub(super) fn new() -> Self {
        Self::default()
    }

    pub(super) fn start(
        &mut self,
        element: ImportElement,
        target_locale: Option<String>,
        unit_id: Option<String>,
    ) -> Result<(), XcStringsError> {
        if self.stack.is_empty() {
            if element.kind != ImportElementKind::Core(CoreElement::Xliff) {
                return self.unexpected_root(&element);
            }
            self.stack.push(Frame {
                element,
                data: FrameData::Root {
                    file_count: 0,
                    extension_needs_file: false,
                },
            });
            return Ok(());
        }

        if matches!(
            self.stack.last().map(|frame| &frame.data),
            Some(FrameData::AnyContent)
        ) {
            self.push(element, FrameData::AnyContent);
            return Ok(());
        }

        if matches!(
            self.stack.last().map(|frame| &frame.data),
            Some(FrameData::Leaf)
        ) {
            return self.unexpected_child(&element);
        }

        if matches!(element.kind, ImportElementKind::Core(_))
            && self.extension_content_blocks(&element)
        {
            return Err(parse_error(format!(
                "element <{}> must not follow extension content inside <{}>",
                element.name,
                self.parent_name()
            )));
        }

        if matches!(
            self.stack.last().map(|frame| &frame.data),
            Some(FrameData::Extension)
        ) && matches!(element.kind, ImportElementKind::Core(_))
        {
            let parent = self.parent_name();
            return Err(parse_error(format!(
                "element <{}> is not allowed as a child of extension element <{parent}>",
                element.name
            )));
        }

        match element.kind {
            ImportElementKind::Core(CoreElement::File) => self.start_file(element, target_locale),
            ImportElementKind::Core(CoreElement::Header) => self.start_header(element),
            ImportElementKind::Core(CoreElement::Body) => self.start_body(element),
            ImportElementKind::Core(CoreElement::Group) => self.start_group(element),
            ImportElementKind::Core(CoreElement::TransUnit) => self.start_unit(element, unit_id),
            ImportElementKind::Core(CoreElement::Source) => self.start_source(element),
            ImportElementKind::Core(CoreElement::Target) => self.start_target(element),
            ImportElementKind::Core(CoreElement::SegSource) => self.start_seg_source(element),
            ImportElementKind::Core(CoreElement::AltTrans) => self.start_alt_trans(element),
            ImportElementKind::Core(CoreElement::BinUnit) => self.start_bin_unit(element, unit_id),
            ImportElementKind::Core(CoreElement::Metadata) => self.start_metadata(element),
            ImportElementKind::Core(CoreElement::Inline) => self.start_inline(element),
            ImportElementKind::Core(CoreElement::Other) => self.start_other_core(element),
            ImportElementKind::Core(CoreElement::Xliff) => Err(parse_error(
                "nested <xliff> element is not allowed".to_string(),
            )),
            ImportElementKind::Extension => self.start_extension(element),
        }
    }

    pub(super) fn end(&mut self, element: ImportElement) -> Result<(), XcStringsError> {
        let Some(frame) = self.stack.pop() else {
            return Err(parse_error(format!(
                "unexpected closing element </{}>",
                element.name
            )));
        };
        if frame.element != element {
            return Err(parse_error(format!(
                "closing element </{}> does not match open <{}>",
                element.name, frame.element.name
            )));
        }

        match frame.data {
            FrameData::Root {
                file_count,
                extension_needs_file,
            } => {
                self.root_file_count = Some(file_count);
                self.root_extension_needs_file = extension_needs_file;
            }
            FrameData::File { phase, .. } if phase != FilePhase::BodySeen => {
                return Err(parse_error(
                    "element <file> is missing required <body> child".to_string(),
                ));
            }
            FrameData::Unit(unit) => self.finish_unit(unit)?,
            FrameData::Group {
                structural_count: 0,
                ..
            } => {
                return Err(parse_error(
                    "element <group> must contain at least one <group>, <trans-unit>, or <bin-unit> child"
                        .to_string(),
                ));
            }
            FrameData::AltTrans {
                phase: AltPhase::Start | AltPhase::Source | AltPhase::SegSource,
            } => {
                return Err(parse_error(
                    "element <alt-trans> must contain exactly one <target> child".to_string(),
                ));
            }
            FrameData::BinUnit {
                phase: BinUnitPhase::NeedSource,
            } => {
                return Err(parse_error(
                    "element <bin-unit> is missing required <bin-source> child".to_string(),
                ));
            }
            FrameData::ChildList { minimum, count, .. } if count < minimum => {
                return Err(parse_error(format!(
                    "element <{}> does not contain its required child",
                    frame.element.name
                )));
            }
            FrameData::BinaryContainer { child_seen: false } => {
                return Err(parse_error(format!(
                    "element <{}> must contain exactly one <internal-file> or <external-file> child",
                    frame.element.name
                )));
            }
            _ => {}
        }
        Ok(())
    }

    pub(super) fn text(&mut self, text: &str) {
        let capture_target = self.stack.iter().rev().find_map(|frame| match frame.data {
            FrameData::Target { main_unit } => Some(main_unit),
            FrameData::Source => Some(false),
            _ => None,
        });
        if capture_target != Some(true) {
            return;
        }
        if let Some(Frame {
            data: FrameData::Unit(unit),
            ..
        }) = self
            .stack
            .iter_mut()
            .rev()
            .find(|frame| matches!(frame.data, FrameData::Unit(_)))
            && let Some(target) = &mut unit.target
        {
            target.push_str(text);
        }
    }

    pub(super) fn finish(self) -> Result<(String, Vec<CompletedTranslation>), XcStringsError> {
        if !self.stack.is_empty() {
            return Err(parse_error(
                "XLIFF structural stack is not closed".to_string(),
            ));
        }
        if self.root_file_count == Some(0) {
            return Err(parse_error(
                "element <xliff> must contain at least one <file> child".to_string(),
            ));
        }
        if self.root_extension_needs_file {
            return Err(parse_error(
                "extension elements at <xliff> level must be followed by <file>".to_string(),
            ));
        }
        let locale = self.target_locale.ok_or_else(|| {
            parse_error("missing target-language attribute in <file> element".to_string())
        })?;
        Ok((locale, self.translations))
    }

    fn start_seg_source(&mut self, element: ImportElement) -> Result<(), XcStringsError> {
        if let Some(FrameData::AltTrans { phase }) =
            self.stack.last_mut().map(|frame| &mut frame.data)
        {
            match phase {
                AltPhase::Start | AltPhase::Source => *phase = AltPhase::SegSource,
                _ => {
                    return Err(parse_error(
                        "element <seg-source> is out of order inside <alt-trans>".to_string(),
                    ));
                }
            }
            self.push(element, FrameData::Opaque);
            return Ok(());
        }
        let Some(unit) = self.current_unit_mut() else {
            return self.unexpected_child(&element);
        };
        if unit.phase != UnitPhase::AfterSource {
            return Err(parse_error(
                "element <seg-source> must appear once after <source> and before <target>"
                    .to_string(),
            ));
        }
        unit.phase = UnitPhase::AfterSegSource;
        self.push(element, FrameData::Opaque);
        Ok(())
    }

    fn start_alt_trans(&mut self, element: ImportElement) -> Result<(), XcStringsError> {
        let Some(unit) = self.current_unit_mut() else {
            return self.unexpected_child(&element);
        };
        if unit.phase == UnitPhase::NeedSource {
            return Err(parse_error(
                "element <alt-trans> must follow <source> inside <trans-unit>".to_string(),
            ));
        }
        unit.phase = UnitPhase::Tail;
        self.push(
            element,
            FrameData::AltTrans {
                phase: AltPhase::Start,
            },
        );
        Ok(())
    }

    fn start_bin_unit(
        &mut self,
        element: ImportElement,
        id: Option<String>,
    ) -> Result<(), XcStringsError> {
        if !self.parent_is(CoreElement::Body) && !self.parent_is(CoreElement::Group) {
            return self.unexpected_child(&element);
        }
        if let Some(id) = id {
            self.register_unit_id(&id)?;
        }
        self.mark_group_structural_content();
        self.push(
            element,
            FrameData::BinUnit {
                phase: BinUnitPhase::NeedSource,
            },
        );
        Ok(())
    }

    fn start_inline(&mut self, element: ImportElement) -> Result<(), XcStringsError> {
        let allowed = matches!(
            self.stack.last().map(|frame| &frame.data),
            Some(
                FrameData::Source
                    | FrameData::Target { .. }
                    | FrameData::Inline
                    | FrameData::Opaque
            )
        );
        if !allowed {
            return self.unexpected_child(&element);
        }
        self.push(element, FrameData::Inline);
        Ok(())
    }
}

fn parse_error(message: String) -> XcStringsError {
    XcStringsError::XliffParse(message)
}
