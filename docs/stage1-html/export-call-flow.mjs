// Export the browsable graph as JSON for tooling and the source inventory.
import { readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { runInNewContext } from 'node:vm';

const root = resolve(import.meta.dirname);
const context = { window: {} };
runInNewContext(readFileSync(resolve(root, 'assets/call-flow.js'), 'utf8'), context);
const graph = JSON.parse(JSON.stringify(context.window.STAGE1_CALL_GRAPH));
if (!graph || !graph.nodes?.length || !graph.edges?.length) throw Error('Empty call graph');
writeFileSync(resolve(root, 'assets/data/verified-relationships.json'), JSON.stringify(graph, null, 2) + '\n');
console.log(`Exported ${graph.nodes.length} verified graph nodes and ${graph.edges.length} classified relationships.`);
