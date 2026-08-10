use super::*;

impl ImportState {
    pub(super) fn register_unit_id(&mut self, id: &str) -> Result<(), XcStringsError> {
        let file_index = self
            .stack
            .iter()
            .rposition(|frame| matches!(frame.data, FrameData::File { .. }))
            .ok_or_else(|| parse_error("XLIFF unit is not enclosed by <file>".to_string()))?;

        let FrameData::File { unit_ids, .. } = &mut self.stack[file_index].data else {
            return Err(parse_error(
                "XLIFF unit is not enclosed by <file>".to_string(),
            ));
        };
        if unit_ids.contains(id) {
            return Err(parse_error(format!(
                "duplicate XLIFF unit id '{id}' inside <file>"
            )));
        }
        if self.document_unit_ids.contains(id) {
            return Err(parse_error(format!(
                "XLIFF unit id '{id}' is repeated across <file> elements and cannot be flattened safely"
            )));
        }

        // Both indexes outlive the current unit frame, so each owns its key.
        unit_ids.insert(id.to_string());
        self.document_unit_ids.insert(id.to_string());
        Ok(())
    }
}
