use super::*;

impl ImportState {
    pub(super) fn start_file(
        &mut self,
        element: ImportElement,
        locale: Option<String>,
    ) -> Result<(), XcStringsError> {
        if !self.parent_is(CoreElement::Xliff) {
            return self.unexpected_child(&element);
        }
        let locale = locale.ok_or_else(|| {
            parse_error("missing target-language attribute in <file> element".to_string())
        })?;
        if locale.trim().is_empty() {
            return Err(parse_error(
                "attribute target-language on <file> must not be empty".to_string(),
            ));
        }
        if let Some(expected) = &self.target_locale
            && expected != &locale
        {
            return Err(parse_error(format!(
                "multiple <file> elements use different target-language values: '{expected}' and '{locale}'"
            )));
        }
        self.target_locale.get_or_insert_with(|| locale.clone());
        if let Some(Frame {
            data:
                FrameData::Root {
                    file_count,
                    extension_needs_file,
                },
            ..
        }) = self.stack.last_mut()
        {
            *file_count += 1;
            *extension_needs_file = false;
        }
        self.stack.push(Frame {
            element,
            data: FrameData::File {
                locale,
                phase: FilePhase::Start,
                unit_ids: HashSet::new(),
            },
        });
        Ok(())
    }

    pub(super) fn start_header(&mut self, element: ImportElement) -> Result<(), XcStringsError> {
        let Some(Frame {
            data: FrameData::File { phase, .. },
            ..
        }) = self.stack.last_mut()
        else {
            return self.unexpected_child(&element);
        };
        match phase {
            FilePhase::Start => *phase = FilePhase::HeaderSeen,
            FilePhase::HeaderSeen => {
                return Err(parse_error(
                    "element <file> may contain at most one <header> child".to_string(),
                ));
            }
            FilePhase::BodySeen => {
                return Err(parse_error(
                    "element <header> must appear before <body> inside <file>".to_string(),
                ));
            }
        }
        self.push(
            element,
            FrameData::Header {
                phase: HeaderPhase::Start,
            },
        );
        Ok(())
    }

    pub(super) fn start_body(&mut self, element: ImportElement) -> Result<(), XcStringsError> {
        let Some(Frame {
            data: FrameData::File { phase, .. },
            ..
        }) = self.stack.last_mut()
        else {
            return self.unexpected_child(&element);
        };
        if *phase == FilePhase::BodySeen {
            return Err(parse_error(
                "element <file> may contain exactly one <body> child".to_string(),
            ));
        }
        *phase = FilePhase::BodySeen;
        self.push(element, FrameData::Body);
        Ok(())
    }

    pub(super) fn start_group(&mut self, element: ImportElement) -> Result<(), XcStringsError> {
        if !self.parent_is(CoreElement::Body) && !self.parent_is(CoreElement::Group) {
            return self.unexpected_child(&element);
        }
        self.mark_group_structural_content();
        self.push(
            element,
            FrameData::Group {
                phase: GroupPhase::Start,
                structural_count: 0,
            },
        );
        Ok(())
    }

    pub(super) fn start_unit(
        &mut self,
        element: ImportElement,
        id: Option<String>,
    ) -> Result<(), XcStringsError> {
        if !self.parent_is(CoreElement::Body)
            && !self.parent_is(CoreElement::Group)
            && !self.parent_is(CoreElement::BinUnit)
        {
            if self.parent_is(CoreElement::Xliff) {
                return Err(parse_error(
                    "element <trans-unit> is not allowed as a child of <xliff>; expected <file>"
                        .to_string(),
                ));
            }
            return self.unexpected_child(&element);
        }
        if let Some(FrameData::BinUnit { phase }) =
            self.stack.last_mut().map(|frame| &mut frame.data)
        {
            if *phase == BinUnitPhase::NeedSource {
                return Err(parse_error(
                    "element <trans-unit> must follow <bin-source> inside <bin-unit>".to_string(),
                ));
            }
            *phase = BinUnitPhase::Tail;
        }
        self.mark_group_structural_content();
        let id = id.ok_or_else(|| {
            parse_error("missing id attribute in <trans-unit> element".to_string())
        })?;
        if id.contains("|==|") {
            return Err(parse_error(format!(
                "Apple XLIFF variation unit id '{id}' is unsupported; import simple stringUnit ids only"
            )));
        }
        self.register_unit_id(&id)?;
        let locale = self.enclosing_file_locale()?.to_string();
        self.push(
            element,
            FrameData::Unit(UnitData {
                id,
                locale,
                phase: UnitPhase::NeedSource,
                target: None,
            }),
        );
        Ok(())
    }

    pub(super) fn start_source(&mut self, element: ImportElement) -> Result<(), XcStringsError> {
        if let Some(FrameData::AltTrans { phase }) =
            self.stack.last_mut().map(|frame| &mut frame.data)
        {
            if *phase != AltPhase::Start {
                return Err(parse_error(
                    "element <source> is out of order inside <alt-trans>".to_string(),
                ));
            }
            *phase = AltPhase::Source;
            self.push(element, FrameData::Source);
            return Ok(());
        }
        let Some(unit) = self.current_unit_mut() else {
            return self.unexpected_child(&element);
        };
        if unit.phase != UnitPhase::NeedSource {
            return Err(parse_error(
                "element <trans-unit> must contain exactly one <source> child".to_string(),
            ));
        }
        unit.phase = UnitPhase::AfterSource;
        self.push(element, FrameData::Source);
        Ok(())
    }

    pub(super) fn start_target(&mut self, element: ImportElement) -> Result<(), XcStringsError> {
        if let Some(FrameData::AltTrans { phase }) =
            self.stack.last_mut().map(|frame| &mut frame.data)
        {
            match phase {
                AltPhase::Start | AltPhase::Source | AltPhase::SegSource => {
                    *phase = AltPhase::Target;
                }
                _ => {
                    return Err(parse_error(
                        "element <alt-trans> must contain exactly one <target> child".to_string(),
                    ));
                }
            }
            self.push(element, FrameData::Target { main_unit: false });
            return Ok(());
        }
        let Some(unit) = self.current_unit_mut() else {
            return self.unexpected_child(&element);
        };
        if unit.target.is_some() {
            return Err(parse_error(
                "element <trans-unit> may contain at most one <target> child".to_string(),
            ));
        }
        match unit.phase {
            UnitPhase::NeedSource => {
                return Err(parse_error(
                    "element <target> must follow <source> inside <trans-unit>".to_string(),
                ));
            }
            UnitPhase::AfterSource | UnitPhase::AfterSegSource => {
                unit.phase = UnitPhase::AfterTarget;
                unit.target = Some(String::new());
            }
            UnitPhase::AfterTarget | UnitPhase::Tail | UnitPhase::Extensions => {
                return Err(parse_error(
                    "element <target> must appear before metadata inside <trans-unit>".to_string(),
                ));
            }
        }
        self.push(element, FrameData::Target { main_unit: true });
        Ok(())
    }
}
