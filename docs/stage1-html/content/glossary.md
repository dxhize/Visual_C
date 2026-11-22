# 66 · Terminology and glossary

> **In one sentence:** Use this as a quick translation when you meet a technical word elsewhere. Return to its linked chapter for the full example.

## Key terms

| Term | Plain meaning |
|---|---|
| **Evidence** | Something actually recorded or statically described, with a source and limits. |
| **Inference** | A conclusion drawn *from* evidence. It may be useful, but it should never be relabeled as a directly recorded fact. |

> **Example.** GDB recorded `raw_bytes_hex=0a000000`. 'Four bytes were read' describes evidence. 'The C initializer ran' would be an inference that those bytes alone do not support.
>
> **What not to assume:** A word such as 'complete' always needs a scope: complete for which processes, time interval and kind of record?

## The big picture

The same numeric value can identify different things at different boundaries. Start with a type's owner and scope before joining two records.

## Recording terms

| Term | Meaning |
|---|---|
| Checkpoint | Retained source-level snapshot at one observed stop, not every executed instruction. |
| Stop ID | Debugger stop counter, not a snapshot vector position. |
| Attempted instruction | An attempt to record/advance at a PC; retention and gaps have separate accounting. |
| Retained event | Actual indexed record in the instruction-event array. |
| Gap | Explicit interval of execution not recorded as exact instructions. |
| Allocation lifetime | Wrapper-derived, scoped identity; an address may be reused by another lifetime. |

### Evidence terms

| Term | Meaning |
|---|---|
| Structural finding | Imported trace contradicts a supported invariant; not simply an incomplete run. |
| Partial | Some observation scope is absent, capped or interrupted. |
| Unavailable | Evidence does not establish the requested relationship. |
| Candidate | Compatible evidence without an exact proof under the documented scope. |
| `EvidenceRef` | Citation bound to the same canonical trace owner and original record position. |
| Source-site candidate | Shared source file/line from an independent process; no primary allocation ID. |

## Continue reading

See [identity domains](identity-contracts.html), [schema](schema.html) and [public API](api.html) for concrete fields and confidence qualifiers. DOC-2/3 will expand individual terms next to the code paths where they are produced.
