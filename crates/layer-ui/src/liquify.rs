use super::*;
use layer_core::{Edit, RecordChange, SourceTarget, liquify::Context, raster::RasterRevision};
use std::sync::Arc;

#[cfg(test)]
#[path = "liquify_tests.rs"]
mod tests;

pub(super) struct Adjustment {
    target: SourceTarget,
    context: Arc<Context>,
    alpha_locked: bool,
    revision: RasterRevision,
    amount: f32,
    checkpoint: Option<u64>,
    gesture: Option<f32>,
}

impl Adjustment {
    fn valid(&self, document: &layer_core::Document, checkpoint: u64) -> bool {
        document.working.target == Some(self.target) && document.target_raster(self.target).is_some_and(|r| r.identity() == self.revision.identity())
            && self.checkpoint.is_none_or(|c| c == checkpoint)
            && document.scene().source_owner(self.target).is_some_and(|owner| !document.is_locked(owner)
                && document.scene().occurrence(owner).is_some_and(|o| o.alpha_locked == self.alpha_locked))
    }
}

impl<R: CanvasRenderer> UiSession<R> {
    pub(super) fn liquify_action(&mut self, reset: bool) -> Result<(), String> {
        self.require_document_idle()?;
        if !reset && self.operation.liquify.take().is_some() { self.refresh_tools(); return Ok(()); }
        let (target, context, alpha_locked) = self.engine.liquify_context().or_else(|| self.operation.liquify.as_ref()
            .filter(|a| a.valid(self.engine.document(), self.engine.checkpoint()))
            .map(|a| (a.target, a.context.clone(), a.alpha_locked)))
            .ok_or_else(|| self.localization().text(MessageId::COMMANDS_LIQUIFY_NO_CHANGES).to_string())?;
        if reset {
            let SourceTarget::Paint(handle) = target else { unreachable!() };
            let edit = Edit::Paint(RecordChange::replace(&self.engine.document().artwork.paint, handle,
                Some((*context.original).clone())).map_err(error)?);
            self.operation.liquify = None;
            self.layer_edit(edit)?;
        } else {
            self.operation.liquify = Some(Adjustment {
                target, revision: context.current.raster.clone(), context, alpha_locked, amount: 1., checkpoint: None, gesture: None,
            });
        }
        self.refresh_tools();
        Ok(())
    }

    pub(super) fn adjust_liquify(&mut self, amount: f32) -> Result<(), String> {
        let adjustment = self.operation.liquify.as_mut().ok_or("Open Liquify Adjust first")?;
        if amount == adjustment.amount { return Ok(()); }
        let preview = adjustment.gesture.is_some();
        if preview {
            self.engine.preview_liquify(adjustment.target, &adjustment.context,
                adjustment.alpha_locked, amount, &adjustment.revision).map_err(error)?;
        } else {
            adjustment.revision = self.engine.adjust_liquify(adjustment.target, &adjustment.context,
                adjustment.alpha_locked, amount, &adjustment.revision, adjustment.checkpoint).map_err(error)?;
            adjustment.checkpoint = Some(self.engine.checkpoint());
        }
        adjustment.amount = amount;
        if !preview { self.refresh_document(); }
        self.refresh_tools();
        Ok(())
    }

    pub(super) fn liquify_contact(&mut self, phase: ContactPhase) -> Result<(), String> {
        if phase == ContactPhase::Down { self.require_document_idle()?; }
        let Some(adjustment) = self.operation.liquify.as_mut() else { return Ok(()); };
        match phase {
            ContactPhase::Down => { adjustment.gesture.get_or_insert(adjustment.amount); }
            ContactPhase::Move => return Ok(()),
            ContactPhase::Up | ContactPhase::Cancel => {
                let Some(original) = adjustment.gesture.take() else { return Ok(()); };
                let amount = adjustment.amount;
                adjustment.amount = original;
                self.engine.clear_liquify_preview();
                if phase == ContactPhase::Up { return self.adjust_liquify(amount); }
            }
        }
        self.refresh_tools();
        Ok(())
    }

    pub(super) fn finish_liquify_contact(&mut self, action: &UiAction) -> Result<(), String> {
        if action.is_host_report() || matches!(action,
            UiAction::ToolSettingContact { id, .. } | UiAction::SetToolSetting { id, .. }
            | UiAction::StepToolSetting { id, .. } | UiAction::ResetToolSetting { id } if id == "liquify_amount") { return Ok(()); }
        if self.operation.liquify.as_ref().is_some_and(|a| a.gesture.is_some()) {
            self.liquify_contact(ContactPhase::Up)?;
        }
        Ok(())
    }

    pub(super) fn liquify_preview_active(&self) -> bool {
        self.operation.liquify.as_ref().is_some_and(|a| a.gesture.is_some())
    }

    pub(super) fn liquify_controls(&mut self) {
        let document = self.engine.document();
        if self.operation.liquify.as_ref().is_some_and(|a| !a.valid(document, self.engine.checkpoint()))
        { self.engine.clear_liquify_preview(); self.operation.liquify = None; }
        if self.layer_interaction.tool != LayerCanvasTool::Paint
            || self.engine.configured_brush().execution != layer_core::BrushExecution::Liquify { return; }
        self.state.tool_actions.extend([CommandId::LiquifyAdjust, CommandId::LiquifyReset]
            .map(|command| ToolSettingAction { command, checkable: command == CommandId::LiquifyAdjust }));
        if let Some(adjustment) = &self.operation.liquify {
            let label_id = MessageId::TOOL_CONTROL_LIQUIFY_AMOUNT;
            self.state.tool_settings.push(ToolSetting { preview: true, id: "liquify_amount", label_id,
                label: self.state.localization.text(label_id), group_id: "", group: "".into(),
                numeric: NumericControl::percent(), value: adjustment.amount });
        }
    }
}
