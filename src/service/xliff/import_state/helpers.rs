use super::*;

impl ImportState {
    pub(super) fn finish_unit(&mut self, unit: UnitData) -> Result<(), XcStringsError> {
        if unit.phase == UnitPhase::NeedSource {
            return Err(parse_error(
                "element <trans-unit> is missing required <source> child".to_string(),
            ));
        }
        let file_index = self
            .stack
            .iter()
            .rev()
            .find_map(|frame| match frame.data {
                FrameData::File { file_index, .. } => Some(file_index),
                _ => None,
            })
            .ok_or_else(|| parse_error("XLIFF unit is not enclosed by <file>".into()))?;
        self.files[file_index].units.push(XliffUnit {
            id: unit.id,
            source: unit.source,
            target: unit.target,
            state: unit.state,
            state_qualifier: unit.state_qualifier,
            notes: unit.notes,
        });
        Ok(())
    }

    pub(super) fn current_unit_mut(&mut self) -> Option<&mut UnitData> {
        match self.stack.last_mut().map(|frame| &mut frame.data) {
            Some(FrameData::Unit(unit)) => Some(unit),
            _ => None,
        }
    }

    pub(super) fn mark_group_structural_content(&mut self) {
        if let Some(Frame {
            data:
                FrameData::Group {
                    phase,
                    structural_count,
                },
            ..
        }) = self.stack.last_mut()
        {
            *phase = GroupPhase::Structural;
            *structural_count += 1;
        }
    }

    pub(super) fn extension_content_blocks(&self, element: &ImportElement) -> bool {
        match self.stack.last().map(|frame| &frame.data) {
            Some(FrameData::Group {
                phase: GroupPhase::Extensions,
                ..
            }) => !matches!(
                element.kind,
                ImportElementKind::Core(
                    CoreElement::Group | CoreElement::TransUnit | CoreElement::BinUnit
                )
            ),
            Some(
                FrameData::Header {
                    phase: HeaderPhase::Extensions,
                }
                | FrameData::AltTrans {
                    phase: AltPhase::Extensions,
                }
                | FrameData::Unit(UnitData {
                    phase: UnitPhase::Extensions,
                    ..
                })
                | FrameData::BinUnit {
                    phase: BinUnitPhase::Extensions,
                },
            ) => true,
            _ => false,
        }
    }

    pub(super) fn parent_is(&self, kind: CoreElement) -> bool {
        self.stack.last().is_some_and(
            |frame| matches!(frame.element.kind, ImportElementKind::Core(parent) if parent == kind),
        )
    }

    pub(super) fn parent_name(&self) -> &str {
        self.stack
            .last()
            .map_or("document", |frame| frame.element.name.as_str())
    }

    pub(super) fn unexpected_root<T>(&self, element: &ImportElement) -> Result<T, XcStringsError> {
        Err(parse_error(format!(
            "document root must be <xliff>; found <{}>",
            element.name
        )))
    }

    pub(super) fn unexpected_child<T>(&self, element: &ImportElement) -> Result<T, XcStringsError> {
        Err(parse_error(format!(
            "element <{}> is not allowed as a child of <{}>",
            element.name,
            self.parent_name()
        )))
    }

    pub(super) fn push(&mut self, element: ImportElement, data: FrameData) {
        self.stack.push(Frame { element, data });
    }
}
