# Unified Processor Architecture

## The Same-Source Processing Policy

The `unified-processor` is designed around a strict architectural boundary: **The Same-Source Policy**. 

When the processor receives a request to parse a codebase, document, or URL crawl, it extracts text, chunks it, generates vector embeddings, and extracts structural relationships. However, it *never* attempts to draw relationships between chunks belonging to different sources. 

By strictly isolating the processing to a single source at a time, `unified-processor`:
1. Can scale horizontally without needing to load or query the global state of the entire knowledge graph.
2. Prevents performance bottlenecks associated with querying massive datasets during ingestion.
3. Builds highly cohesive "islands" of knowledge (subgraphs) that perfectly map to the original structure of the document or codebase.

*Note: Cross-source relationship discovery is handled asynchronously by the `fast-fetcher` microservice after the fact.*

## The SourceRelationshipRouter

The core of the graph building logic lives in `src/graph/extractors/mod.rs` inside the `SourceRelationshipRouter`.

```rust
pub struct SourceRelationshipRouter {
    hierarchical: HierarchicalExtractor,
    code: CodeExtractor,
    document: DocumentExtractor,
    web: WebExtractor,
    conversation: ConversationExtractor,
    schema: SchemaExtractor,
    transcript: TranscriptExtractor,
}
```

When a batch of chunks finishes processing, `extract_all(&self, chunks: &[Chunk])` is called. It passes the current batch of chunks through each specialized extractor. 

Each extractor only looks for patterns within the provided `chunks` array (which all belong to the current `source_id`). 

### Example Extractors:
- **HierarchicalExtractor**: Detects `PARENT_OF` and `SIBLING_OF` relationships based on the file hierarchy or document outline of the current source.
- **CodeExtractor**: Extracts `CALLS`, `INHERITS`, and `IMPORTS` relationships between code chunks in the current repository.
- **DocumentExtractor**: Connects sections of the same document via `REFERENCES` edges.

## Symbol Resolution

For code repositories, the `SymbolIndex` (`src/graph/resolver/symbol.rs`) builds a temporary, in-memory index of all functions, classes, and types defined in the *current* batch of chunks. It then resolves cross-file function calls and instantiations into deterministic graph edges. Because the index is built strictly from the current ingestion batch, it guarantees that no cross-source edges are accidentally created.

## Storage Layer

The `FalkorDBStorage` (`src/storage/falkordb.rs`) adapter natively interfaces with FalkorDB using Cypher queries. 

- It stores `Vector_Chunk` nodes for every extracted chunk.
- It stores `Code_Entity`, `Web_Page`, and `Repository` metadata nodes.
- It writes the structural edges discovered by the `SourceRelationshipRouter` directly into the graph.
