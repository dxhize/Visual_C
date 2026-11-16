//! Structural validation of recorded traces (not simulated state).
//! V2.1 note: register replay machinery was removed together with per-instruction
//! register capture; this module now validates trace structure only.
use crate::models::*;

pub fn validate(trace: &DebugSessionTrace) -> Result<(), String> {
    let data = &trace.recording;
    for (position, event) in data.instruction_events.iter().enumerate() {
        if event.event_id != position as u64 {
            return Err("noncontiguous instruction IDs".into());
        }
        if let Some(id) = event.instruction_id {
            let instruction = data
                .disassembly_table
                .get(id as usize)
                .ok_or("invalid disassembly reference")?;
            if instruction.address != event.address {
                return Err("instruction address/reference mismatch".into());
            }
        }
        if event
            .checkpoint_id
            .is_some_and(|id| id >= trace.snapshots.len())
        {
            return Err("invalid checkpoint reference".into());
        }
    }
    if !data.instruction_events.is_empty()
        && data.instruction_count != data.instruction_events.len() as u64
    {
        return Err("instruction count mismatch".into());
    }
    for id in &data.source_checkpoints {
        let checkpoint = trace
            .snapshots
            .get(*id)
            .ok_or("invalid source checkpoint")?;
        if checkpoint.observation.instruction_position > data.instruction_count {
            return Err("checkpoint beyond timeline".into());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn event(id: u64) -> InstructionEvent {
        InstructionEvent {
            stop_id_before: id,
            stop_id_after: Some(id + 1),
            event_id: id,
            instruction_id: None,
            address: "0x1000".into(),
            next_address: Some("0x1001".into()),
            thread_id: Some("1".into()),
            pid: Some(1),
            source: SourceContext::default(),
            checkpoint_id: None,
            outcome: "completed".into(),
            stopped_reason: None,
            signal: None,
            memory: Vec::new(),
            registers: None,
        }
    }
    #[test]
    fn broken_reference_and_duplicate_event_id_are_rejected() {
        let mut trace = DebugSessionTrace::default();
        trace.recording.instruction_events = vec![event(1)];
        assert!(validate(&trace).is_err());
        trace.recording.instruction_events[0].event_id = 0;
        trace.recording.instruction_events[0].instruction_id = Some(999);
        assert!(validate(&trace).is_err());
    }
}
