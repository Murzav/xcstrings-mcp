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
        unit_ids.insert(id.to_string());
        Ok(())
    }
}
