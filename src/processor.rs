//! Deterministic replay and structural validation of recorded (not simulated) state.
use crate::models::*;
use std::collections::BTreeMap;

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

/// Registers immediately after a recorded attempt. Missing/incomplete observations
/// return an error rather than stale values mislabeled exact.
pub fn replay_registers(
    trace: &DebugSessionTrace,
    event_id: u64,
) -> Result<BTreeMap<u32, RegisterValue>, String> {
    let target = trace
        .recording
        .instruction_events
        .get(event_id as usize)
        .ok_or("event not resident; load journal")?;
    let thread = &target.thread_id;
    let mut state = BTreeMap::new();
    let mut valid = false;
    let start = trace
        .recording
        .instruction_events
        .iter()
        .take(event_id as usize + 1)
        .rposition(|e| {
            &e.thread_id == thread
                && e.registers
                    .as_ref()
                    .is_some_and(|r| r.complete && r.full_checkpoint)
        })
        .ok_or("no valid register checkpoint")?;
    for event in trace
        .recording
        .instruction_events
        .iter()
        .take(event_id as usize + 1)
        .skip(start)
        .filter(|e| &e.thread_id == thread)
    {
        match &event.registers {
            Some(observation) if observation.complete => {
                if observation.full_checkpoint {
                    state.clear();
                    valid = true;
                }
                if valid {
                    for value in &observation.values {
                        state.insert(value.number, value.clone());
                    }
                }
            }
            _ => valid = false,
        }
    }
    if valid {
        Ok(state)
    } else {
        Err("exact register state unavailable at this position".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn event(id: u64, full: bool, value: Option<&str>) -> InstructionEvent {
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
            registers: Some(RegisterObservation {
                full_checkpoint: full,
                architecture: "test-architecture".into(),
                thread_id: Some("1".into()),
                complete: true,
                error: None,
                values: vec![RegisterValue {
                    number: 0,
                    name: "r".into(),
                    value: value.map(str::to_owned),
                    availability: if value.is_some() {
                        DataValidity::Valid
                    } else {
                        DataValidity::Unavailable
                    },
                }],
            }),
        }
    }
    #[test]
    fn deltas_reconstruct_and_unavailable_invalidates_previous_values() {
        let mut trace = DebugSessionTrace::default();
        trace.recording.instruction_events = vec![
            event(0, true, Some("0x1")),
            event(1, false, Some("0x2")),
            event(2, false, None),
        ];
        trace.recording.instruction_count = 3;
        validate(&trace).unwrap();
        assert_eq!(
            replay_registers(&trace, 1).unwrap()[&0].value.as_deref(),
            Some("0x2")
        );
        assert_eq!(replay_registers(&trace, 2).unwrap()[&0].value, None);
        trace.recording.instruction_events[1].registers = None;
        assert!(replay_registers(&trace, 2).is_err());
        trace.recording.instruction_events[2]
            .registers
            .as_mut()
            .unwrap()
            .full_checkpoint = true;
        assert!(replay_registers(&trace, 2).is_ok());
    }
    #[test]
    fn broken_reference_and_duplicate_event_id_are_rejected() {
        let mut trace = DebugSessionTrace::default();
        trace.recording.instruction_events = vec![event(1, true, Some("0x1"))];
        assert!(validate(&trace).is_err());
        trace.recording.instruction_events[0].event_id = 0;
        trace.recording.instruction_events[0].instruction_id = Some(999);
        assert!(validate(&trace).is_err());
    }
}
