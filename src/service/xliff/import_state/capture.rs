use super::*;
use crate::service::xliff::normalized_attribute;
use quick_xml::events::BytesStart;

impl ImportState {
    /// Capture semantics only after the shared XML grammar accepted this frame.
    pub(in crate::service::xliff) fn attributes(
        &mut self,
        element: &BytesStart<'_>,
    ) -> Result<(), XcStringsError> {
        let Some(frame) = self.stack.last() else {
            return Ok(());
        };
        if let FrameData::File { file_index, .. } = frame.data {
            let file = &mut self.files[file_index];
            file.original = normalized_attribute(element, "original")?;
            file.source_language = normalized_attribute(element, "source-language")?;
        }
        let main_target = matches!(frame.data, FrameData::Target { main_unit: true });
        let direct_note = frame.element.name == "note"
            && self
                .stack
                .iter()
                .rev()
                .nth(1)
                .is_some_and(|parent| matches!(parent.data, FrameData::Unit(_)));
        if main_target || direct_note {
            let unit = self.stack.iter_mut().rev().find_map(|f| match &mut f.data {
                FrameData::Unit(unit) => Some(unit),
                _ => None,
            });
            if let Some(unit) = unit {
                if main_target {
                    unit.state = normalized_attribute(element, "state")?;
                    unit.state_qualifier = normalized_attribute(element, "state-qualifier")?;
                } else {
                    unit.notes.push(XliffNote {
                        text: String::new(),
                        from: normalized_attribute(element, "from")?,
                    });
                }
            }
        }
        Ok(())
    }

    pub(in crate::service::xliff) fn text(&mut self, text: &str) {
        let Some(unit_index) = self
            .stack
            .iter()
            .rposition(|f| matches!(f.data, FrameData::Unit(_)))
        else {
            return;
        };
        let tail = &self.stack[unit_index + 1..];
        if tail
            .iter()
            .any(|f| matches!(f.data, FrameData::AltTrans { .. } | FrameData::Extension))
        {
            return;
        }
        let target = tail
            .iter()
            .any(|f| matches!(f.data, FrameData::Target { main_unit: true }));
        let source = tail.iter().any(|f| matches!(f.data, FrameData::Source));
        let note = tail.first().is_some_and(|f| f.element.name == "note");
        if let FrameData::Unit(unit) = &mut self.stack[unit_index].data {
            if target {
                if let Some(value) = &mut unit.target {
                    value.push_str(text);
                }
            } else if source {
                if let Some(source) = &mut unit.source {
                    source.push_str(text);
                }
            } else if note && let Some(note) = unit.notes.last_mut() {
                note.text.push_str(text);
            }
        }
    }
}
