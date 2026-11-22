(function () {
  "use strict";
  window.STAGE1_GUIDED_PATHS = [
    {
      id: "cli-export",
      title: "CLI to successful trace export",
      summary: "Follow configuration, compilation, debugger recording, trace assembly and the final bounded JSON document.",
      nodes: [
        { id: "main", note: "Parse CLI arguments and own process exit policy.", via: "entry" },
        { id: "cli.config", note: "Merge defaults, JSON configuration and explicit CLI overrides.", via: "direct call" },
        { id: "trace.program", note: "Create the supervised session scope.", via: "direct call" },
        { id: "trace.inner", note: "Probe tools, compile, extract static metadata and run the debugger phase.", via: "direct call" },
        { id: "compiler.project", note: "Build the wrapped debug binary and optional diagnostic artifacts.", via: "direct call" },
        { id: "trace.gdb", note: "Run the observed stop loop and assemble the trace.", via: "direct call" },
        { id: "trace.snapshot", note: "Append a retained checkpoint snapshot within budgets.", via: "direct call" },
        { id: "export.json", note: "Serialize the completed trace with an exact byte preflight.", via: "direct call" },
        { id: "export.atomic", note: "Commit optional file output before writing stdout.", via: "direct call" }
      ]
    },
    {
      id: "gdb-checkpoint",
      title: "GDB stop to checkpoint",
      summary: "Trace one asynchronous MI stop through parsing, state capture and recorder retention.",
      nodes: [
        { id: "trace.gdb", note: "Wait for an observed stop after an execution command.", via: "execution command" },
        { id: "gdb.reader", note: "Read complete GDB/MI2 stdout lines in the reader task.", via: "data handoff" },
        { id: "gdb.handle", note: "Classify a line as a result, async stop, notification or stream.", via: "direct call" },
        { id: "mi.parse", note: "Decode MI syntax into a typed record.", via: "direct call" },
        { id: "trace.capture", note: "Capture frame, variables, callers and bounded memory.", via: "direct call" },
        { id: "gdb.capture", note: "Build a CapturedState from the selected frame.", via: "direct call" },
        { id: "trace.snapshot", note: "Append the checkpoint snapshot and source-checkpoint position.", via: "direct call" },
        { id: "recorder.checkpoint", note: "Reserve, journal and retain the snapshot record.", via: "direct call" }
      ]
    },
    {
      id: "malloc-lifetime",
      title: "Allocation wrapper to heap lifetime",
      summary: "Cross the C/Rust file boundary from a linked malloc wrapper to a consumer allocation page.",
      nodes: [
        { id: "c.wrap_malloc", note: "Call the real allocator once and update the PID-local table.", via: "linked wrapper" },
        { id: "c.emit", note: "Format a PID-local sequenced event under the allocator lock.", via: "direct C call" },
        { id: "c.write_all", note: "Append a complete event line; failures are recorded.", via: "direct C call" },
        { id: "tracker.read", note: "Read only complete rows from the event file.", via: "file data handoff" },
        { id: "heap.ingest", note: "Verify the per-PID prefix and update block state.", via: "direct call" },
        { id: "trace.snapshot", note: "Read alive observed blocks into the checkpoint.", via: "data handoff" },
        { id: "export.json", note: "Serialize heap events and lifetimes.", via: "serialization" },
        { id: "store.open", note: "Load the JSON trace into the canonical parser store.", via: "file data handoff" },
        { id: "idx.alloc", note: "Index events/state/lifetimes by allocation ID.", via: "direct call" },
        { id: "store.heap_alloc", note: "Look up all events for a trace-local allocation ID.", via: "direct call" },
        { id: "sem.heap", note: "Join lifetime, state, history and realloc confidence.", via: "direct call" },
        { id: "consumer.heap_events", note: "Return a bounded page of borrowed allocation events.", via: "direct call" }
      ]
    },
    {
      id: "dwarf-type",
      title: "DWARF extraction to type registry",
      summary: "Follow GCC debug metadata through bounded DIE resolution into a parser type lookup.",
      nodes: [
        { id: "trace.inner", note: "Request static metadata from the compiled debug artifact.", via: "orchestration" },
        { id: "dwarf.extract", note: "Open the ELF and enumerate compilation units and DIEs.", via: "direct call" },
        { id: "dwarf.load", note: "Build CU and type/declaration indexes.", via: "direct call" },
        { id: "dwarf.index", note: "Index declaration and type references for association.", via: "direct call" },
        { id: "dwarf.origin", note: "Resolve specification/abstract-origin chains with a cycle/hop bound.", via: "direct call" },
        { id: "dwarf.associate", note: "Link unique runtime declarations to static DIE evidence.", via: "direct call" },
        { id: "fusion.fuse", note: "Add bounded cross-source references after runtime assembly.", via: "direct call" },
        { id: "export.json", note: "Serialize optional type/declaration registries.", via: "serialization" },
        { id: "store.open", note: "Load the trace and build parser metadata indexes.", via: "file data handoff" },
        { id: "store.type_entry", note: "Borrow a trace-local type record without decoding Stage 2 memory.", via: "direct call" }
      ]
    },
    {
      id: "json-summary",
      title: "JSON input to checkpoint summary",
      summary: "Follow parser ownership from raw bytes to a bounded consumer summary.",
      nodes: [
        { id: "p.open", note: "Open and gate a schema-2 JSON document.", via: "entry" },
        { id: "p.assemble", note: "Own the canonical Arc trace and shared indexes.", via: "direct call" },
        { id: "idx.build", note: "Build position-only execution/source/allocation/metadata indexes.", via: "direct call" },
        { id: "store.input", note: "Borrow the store through SemanticInputView.", via: "direct call" },
        { id: "ctx.checkpoint", note: "Resolve one snapshot into a CheckpointContext.", via: "direct call" },
        { id: "store.validated", note: "Run deep validation once and retain its report.", via: "direct call" },
        { id: "sem.checkpoint", note: "Build checkpoint evidence with stop-local positions.", via: "direct call" },
        { id: "consumer.summary", note: "Compose a snapshot reference and validated view.", via: "direct call" },
        { id: "consumer.vars", note: "Return a bounded page of borrowed variable evidence.", via: "direct call" }
      ]
    },
    {
      id: "validation-confidence",
      title: "Deep validation to semantic confidence",
      summary: "See how explicit findings become relationship-specific confidence without inventing facts.",
      nodes: [
        { id: "store.validate", note: "Run the explicit deep-validation entry point.", via: "entry" },
        { id: "val.deep", note: "Run engine and specialized validation passes.", via: "direct call" },
        { id: "val.instructions", note: "Check event ordering and recorded references.", via: "direct call" },
        { id: "val.snapshots", note: "Check snapshot, caller and pointer claims.", via: "direct call" },
        { id: "val.heap", note: "Audit heap identity, sequence and realloc corroboration.", via: "direct call" },
        { id: "val.builder", note: "Aggregate findings by severity/code with first location.", via: "direct call" },
        { id: "store.validated", note: "Retain the report in a validated semantic view.", via: "data handoff" },
        { id: "sem.qualify", note: "Downgrade all confidence on structural corruption.", via: "direct call" },
        { id: "sem.variable", note: "Attach relationship-specific confidence to a variable observation.", via: "direct call" }
      ]
    }
  ];
}());
