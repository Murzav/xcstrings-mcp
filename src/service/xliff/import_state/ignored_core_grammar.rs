use super::*;

impl ImportState {
    pub(super) fn start_metadata(&mut self, element: ImportElement) -> Result<(), XcStringsError> {
        let (expected_child, minimum) = match element.name.as_str() {
            "context-group" => ("context", 1),
            "count-group" => ("count", 0),
            "note" => return self.start_note(element),
            _ => return self.unexpected_child(&element),
        };
        self.enter_metadata_position(&element)?;
        self.push(
            element,
            FrameData::ChildList {
                expected: expected_child,
                minimum,
                count: 0,
            },
        );
        Ok(())
    }

    pub(super) fn start_other_core(
        &mut self,
        element: ImportElement,
    ) -> Result<(), XcStringsError> {
        match element.name.as_str() {
            "skl" | "glossary" | "reference" => self.start_external_reference(element),
            "phase-group" => self.start_phase_group(element),
            "phase" => self.start_phase(element),
            "tool" => self.start_tool(element),
            "context" | "count" | "prop" => self.start_leaf_child(element),
            "prop-group" => self.start_prop_group(element),
            "bin-source" => self.start_bin_source(element),
            "bin-target" => self.start_bin_target(element),
            "internal-file" | "external-file" => self.start_binary_file(element),
            _ => self.unexpected_child(&element),
        }
    }

    fn start_note(&mut self, element: ImportElement) -> Result<(), XcStringsError> {
        if self.register_list_child(&element)? {
            self.push(element, FrameData::Leaf);
            return Ok(());
        }
        self.enter_metadata_position(&element)?;
        self.push(element, FrameData::Leaf);
        Ok(())
    }

    fn start_prop_group(&mut self, element: ImportElement) -> Result<(), XcStringsError> {
        self.enter_metadata_position(&element)?;
        self.push(
            element,
            FrameData::ChildList {
                expected: "prop",
                minimum: 1,
                count: 0,
            },
        );
        Ok(())
    }

    fn enter_metadata_position(&mut self, element: &ImportElement) -> Result<(), XcStringsError> {
        let allowed_in_header =
            matches!(element.name.as_str(), "count-group" | "prop-group" | "note");
        let allowed_in_alt = matches!(
            element.name.as_str(),
            "context-group" | "prop-group" | "note"
        );
        match self.stack.last_mut().map(|frame| &mut frame.data) {
            Some(FrameData::Header { phase }) if allowed_in_header => {
                *phase = HeaderPhase::Metadata;
            }
            Some(FrameData::AltTrans { phase }) if allowed_in_alt => {
                let allowed = match element.name.as_str() {
                    "context-group" => matches!(phase, AltPhase::Target | AltPhase::Context),
                    "prop-group" => matches!(
                        phase,
                        AltPhase::Target | AltPhase::Context | AltPhase::Property
                    ),
                    "note" => matches!(
                        phase,
                        AltPhase::Target | AltPhase::Context | AltPhase::Property | AltPhase::Note
                    ),
                    _ => false,
                };
                if !allowed {
                    return Err(metadata_order_error(element, "alt-trans"));
                }
                *phase = match element.name.as_str() {
                    "context-group" => AltPhase::Context,
                    "prop-group" => AltPhase::Property,
                    _ => AltPhase::Note,
                };
            }
            Some(FrameData::Group { phase, .. }) => {
                let allowed = match element.name.as_str() {
                    "context-group" => matches!(phase, GroupPhase::Start | GroupPhase::Context),
                    "count-group" => matches!(
                        phase,
                        GroupPhase::Start | GroupPhase::Context | GroupPhase::Count
                    ),
                    "prop-group" => matches!(
                        phase,
                        GroupPhase::Start
                            | GroupPhase::Context
                            | GroupPhase::Count
                            | GroupPhase::Property
                    ),
                    "note" => matches!(
                        phase,
                        GroupPhase::Start
                            | GroupPhase::Context
                            | GroupPhase::Count
                            | GroupPhase::Property
                            | GroupPhase::Note
                    ),
                    _ => false,
                };
                if !allowed {
                    return Err(metadata_order_error(element, "group"));
                }
                *phase = match element.name.as_str() {
                    "context-group" => GroupPhase::Context,
                    "count-group" => GroupPhase::Count,
                    "prop-group" => GroupPhase::Property,
                    _ => GroupPhase::Note,
                };
            }
            Some(FrameData::Unit(unit)) if unit.phase != UnitPhase::NeedSource => {
                unit.phase = UnitPhase::Tail;
            }
            Some(FrameData::BinUnit { phase }) if *phase != BinUnitPhase::NeedSource => {
                *phase = BinUnitPhase::Tail;
            }
            _ => return self.unexpected_child(element),
        }
        Ok(())
    }

    fn start_external_reference(&mut self, element: ImportElement) -> Result<(), XcStringsError> {
        let Some(FrameData::Header { phase }) = self.stack.last_mut().map(|frame| &mut frame.data)
        else {
            return self.unexpected_child(&element);
        };
        match element.name.as_str() {
            "skl" if *phase == HeaderPhase::Start => *phase = HeaderPhase::Skeleton,
            "glossary" | "reference" => *phase = HeaderPhase::Metadata,
            _ => return Err(metadata_order_error(&element, "header")),
        }
        self.push(element, FrameData::BinaryContainer { child_seen: false });
        Ok(())
    }

    fn start_phase_group(&mut self, element: ImportElement) -> Result<(), XcStringsError> {
        let Some(FrameData::Header { phase }) = self.stack.last_mut().map(|frame| &mut frame.data)
        else {
            return self.unexpected_child(&element);
        };
        if !matches!(phase, HeaderPhase::Start | HeaderPhase::Skeleton) {
            return Err(metadata_order_error(&element, "header"));
        }
        *phase = HeaderPhase::PhaseGroup;
        self.push(
            element,
            FrameData::ChildList {
                expected: "phase",
                minimum: 1,
                count: 0,
            },
        );
        Ok(())
    }

    fn start_phase(&mut self, element: ImportElement) -> Result<(), XcStringsError> {
        if !self.register_list_child(&element)? {
            return self.unexpected_child(&element);
        }
        self.push(
            element,
            FrameData::ChildList {
                expected: "note",
                minimum: 0,
                count: 0,
            },
        );
        Ok(())
    }

    fn start_tool(&mut self, element: ImportElement) -> Result<(), XcStringsError> {
        let Some(FrameData::Header { phase }) = self.stack.last_mut().map(|frame| &mut frame.data)
        else {
            return self.unexpected_child(&element);
        };
        *phase = HeaderPhase::Metadata;
        self.push(element, FrameData::AnyContent);
        Ok(())
    }

    fn start_leaf_child(&mut self, element: ImportElement) -> Result<(), XcStringsError> {
        if !self.register_list_child(&element)? {
            return self.unexpected_child(&element);
        }
        self.push(element, FrameData::Leaf);
        Ok(())
    }

    fn register_list_child(&mut self, element: &ImportElement) -> Result<bool, XcStringsError> {
        let Some(FrameData::ChildList {
            expected, count, ..
        }) = self.stack.last_mut().map(|frame| &mut frame.data)
        else {
            return Ok(false);
        };
        if *expected != element.name {
            return self.unexpected_child(element);
        }
        *count += 1;
        Ok(true)
    }

    fn start_bin_source(&mut self, element: ImportElement) -> Result<(), XcStringsError> {
        let Some(FrameData::BinUnit { phase }) = self.stack.last_mut().map(|frame| &mut frame.data)
        else {
            return self.unexpected_child(&element);
        };
        if *phase != BinUnitPhase::NeedSource {
            return Err(parse_error(
                "element <bin-unit> must contain exactly one <bin-source> child".to_string(),
            ));
        }
        *phase = BinUnitPhase::AfterSource;
        self.push(element, FrameData::BinaryContainer { child_seen: false });
        Ok(())
    }

    fn start_bin_target(&mut self, element: ImportElement) -> Result<(), XcStringsError> {
        let Some(FrameData::BinUnit { phase }) = self.stack.last_mut().map(|frame| &mut frame.data)
        else {
            return self.unexpected_child(&element);
        };
        match phase {
            BinUnitPhase::NeedSource => {
                return Err(parse_error(
                    "element <bin-target> must follow <bin-source> inside <bin-unit>".to_string(),
                ));
            }
            BinUnitPhase::AfterSource => *phase = BinUnitPhase::AfterTarget,
            BinUnitPhase::AfterTarget | BinUnitPhase::Tail | BinUnitPhase::Extensions => {
                return Err(parse_error(
                    "element <bin-unit> may contain at most one <bin-target> child".to_string(),
                ));
            }
        }
        self.push(element, FrameData::BinaryContainer { child_seen: false });
        Ok(())
    }

    fn start_binary_file(&mut self, element: ImportElement) -> Result<(), XcStringsError> {
        let Some(FrameData::BinaryContainer { child_seen }) =
            self.stack.last_mut().map(|frame| &mut frame.data)
        else {
            return self.unexpected_child(&element);
        };
        if *child_seen {
            return Err(parse_error(format!(
                "element <{}> must contain exactly one <internal-file> or <external-file> child",
                self.parent_name()
            )));
        }
        *child_seen = true;
        self.push(element, FrameData::Leaf);
        Ok(())
    }
}

fn metadata_order_error(element: &ImportElement, parent: &str) -> XcStringsError {
    parse_error(format!(
        "element <{}> is out of order inside <{parent}>",
        element.name
    ))
}
