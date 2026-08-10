use super::*;

impl ImportState {
    pub(super) fn start_extension(&mut self, element: ImportElement) -> Result<(), XcStringsError> {
        match self.stack.last_mut().map(|frame| &mut frame.data) {
            Some(FrameData::Root {
                extension_needs_file,
                ..
            }) => *extension_needs_file = true,
            Some(FrameData::Header { phase }) => *phase = HeaderPhase::Extensions,
            Some(FrameData::AltTrans { phase })
                if matches!(
                    phase,
                    AltPhase::Target | AltPhase::Context | AltPhase::Property | AltPhase::Note
                ) =>
            {
                *phase = AltPhase::Extensions;
            }
            Some(FrameData::Extension) => {}
            Some(FrameData::Group { phase, .. }) if *phase != GroupPhase::Structural => {
                *phase = GroupPhase::Extensions;
            }
            Some(FrameData::Unit(unit)) if unit.phase != UnitPhase::NeedSource => {
                unit.phase = UnitPhase::Extensions;
            }
            Some(FrameData::BinUnit { phase }) if *phase != BinUnitPhase::NeedSource => {
                *phase = BinUnitPhase::Extensions;
            }
            _ => return self.unexpected_child(&element),
        }
        self.push(element, FrameData::Extension);
        Ok(())
    }
}
