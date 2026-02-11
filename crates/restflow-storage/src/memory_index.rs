use anyhow::{Context, Result};
use parking_lot::Mutex;
use std::path::Path;
use std::sync::Arc;
use tantivy::collector::TopDocs;
use tantivy::doc;
use tantivy::query::{BooleanQuery, Occur, QueryParser, TermQuery};
use tantivy::schema::{Field, IndexRecordOption, STORED, STRING, Schema, TEXT, Value};
use tantivy::{Index, IndexReader, IndexWriter, ReloadPolicy, TantivyDocument, Term};

#[derive(Debug, Clone)]
pub struct IndexableChunk {
    pub id: String,
    pub agent_id: String,
    pub content: String,
    pub tags: Vec<String>,
    pub created_at: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SearchHit {
    pub chunk_id: String,
    pub score: f32,
}

pub struct MemoryIndex {
    index: Index,
    reader: IndexReader,
    writer: Arc<Mutex<IndexWriter>>,
    chunk_id_field: Field,
    agent_id_field: Field,
    content_field: Field,
    tags_field: Field,
    created_at_field: Field,
}

impl MemoryIndex {
    pub fn open(path: &Path) -> Result<Self> {
        std::fs::create_dir_all(path)
            .with_context(|| format!("failed to create index dir: {}", path.display()))?;

        let schema = build_schema();
        let index = Index::open_in_dir(path).or_else(|_| Index::create_in_dir(path, schema))?;
        Self::from_index(index)
    }

    pub fn in_memory() -> Result<Self> {
        let schema = build_schema();
        let index = Index::create_in_ram(schema);
        Self::from_index(index)
    }

    pub fn doc_count(&self) -> Result<u64> {
        Ok(self.reader.searcher().num_docs())
    }

    pub fn index_chunk(&self, chunk: &IndexableChunk) -> Result<()> {
        let mut writer = self.writer.lock();
        writer.delete_term(Term::from_field_text(self.chunk_id_field, &chunk.id));

        let mut document = doc!(
            self.chunk_id_field => chunk.id.clone(),
            self.agent_id_field => chunk.agent_id.clone(),
            self.content_field => chunk.content.clone(),
            self.created_at_field => chunk.created_at,
        );

        for tag in &chunk.tags {
            document.add_text(self.tags_field, tag);
        }

        writer.add_document(document)?;
        writer.commit()?;
        self.reader.reload()?;
        Ok(())
    }

    pub fn remove_chunk(&self, chunk_id: &str) -> Result<()> {
        let mut writer = self.writer.lock();
        writer.delete_term(Term::from_field_text(self.chunk_id_field, chunk_id));
        writer.commit()?;
        self.reader.reload()?;
        Ok(())
    }

    pub fn search(&self, query: &str, agent_id: &str, limit: usize) -> Result<Vec<SearchHit>> {
        let searcher = self.reader.searcher();

        let mut parser =
            QueryParser::for_index(&self.index, vec![self.content_field, self.tags_field]);
        parser.set_conjunction_by_default();

        let text_query = parser.parse_query(query)?;
        let agent_term = Term::from_field_text(self.agent_id_field, agent_id);
        let agent_query = TermQuery::new(agent_term, IndexRecordOption::Basic);

        let combined = BooleanQuery::new(vec![
            (Occur::Must, Box::new(text_query)),
            (Occur::Must, Box::new(agent_query)),
        ]);

        let top_docs = searcher.search(&combined, &TopDocs::with_limit(limit))?;

        let mut hits = Vec::with_capacity(top_docs.len());
        for (score, address) in top_docs {
            let document: TantivyDocument = searcher.doc(address)?;
            let Some(value) = document.get_first(self.chunk_id_field) else {
                continue;
            };
            let Some(chunk_id) = value.as_str() else {
                continue;
            };
            hits.push(SearchHit {
                chunk_id: chunk_id.to_string(),
                score,
            });
        }

        Ok(hits)
    }

    pub fn rebuild<I>(&self, chunks: I) -> Result<usize>
    where
        I: IntoIterator<Item = IndexableChunk>,
    {
        let mut writer = self.writer.lock();
        writer.delete_all_documents()?;

        let mut count = 0usize;
        for chunk in chunks {
            let mut document = doc!(
                self.chunk_id_field => chunk.id,
                self.agent_id_field => chunk.agent_id,
                self.content_field => chunk.content,
                self.created_at_field => chunk.created_at,
            );
            for tag in chunk.tags {
                document.add_text(self.tags_field, &tag);
            }
            writer.add_document(document)?;
            count += 1;
        }

        writer.commit()?;
        self.reader.reload()?;
        Ok(count)
    }

    fn from_index(index: Index) -> Result<Self> {
        let schema = index.schema();
        let chunk_id_field = schema
            .get_field("chunk_id")
            .context("missing chunk_id field in index schema")?;
        let agent_id_field = schema
            .get_field("agent_id")
            .context("missing agent_id field in index schema")?;
        let content_field = schema
            .get_field("content")
            .context("missing content field in index schema")?;
        let tags_field = schema
            .get_field("tags")
            .context("missing tags field in index schema")?;
        let created_at_field = schema
            .get_field("created_at")
            .context("missing created_at field in index schema")?;

        let writer = index.writer(50_000_000)?;
        let reader = index
            .reader_builder()
            .reload_policy(ReloadPolicy::OnCommitWithDelay)
            .try_into()?;

        Ok(Self {
            index,
            reader,
            writer: Arc::new(Mutex::new(writer)),
            chunk_id_field,
            agent_id_field,
            content_field,
            tags_field,
            created_at_field,
        })
    }
}

fn build_schema() -> Schema {
    let mut schema_builder = Schema::builder();
    schema_builder.add_text_field("chunk_id", STRING | STORED);
    schema_builder.add_text_field("agent_id", STRING);
    schema_builder.add_text_field("content", TEXT | STORED);
    schema_builder.add_text_field("tags", STRING);
    schema_builder.add_i64_field("created_at", STORED);
    schema_builder.build()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_index_and_search() {
        let index = MemoryIndex::in_memory().unwrap();

        index
            .index_chunk(&IndexableChunk {
                id: "chunk-1".to_string(),
                agent_id: "agent-a".to_string(),
                content: "Rust async task scheduler".to_string(),
                tags: vec!["rust".to_string(), "async".to_string()],
                created_at: 1,
            })
            .unwrap();

        index
            .index_chunk(&IndexableChunk {
                id: "chunk-2".to_string(),
                agent_id: "agent-a".to_string(),
                content: "Python notebook".to_string(),
                tags: vec!["python".to_string()],
                created_at: 2,
            })
            .unwrap();

        let hits = index.search("rust async", "agent-a", 10).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].chunk_id, "chunk-1");
    }

    #[test]
    fn test_agent_scoped_search() {
        let index = MemoryIndex::in_memory().unwrap();

        index
            .index_chunk(&IndexableChunk {
                id: "chunk-a".to_string(),
                agent_id: "agent-a".to_string(),
                content: "shared keyword".to_string(),
                tags: vec![],
                created_at: 1,
            })
            .unwrap();

        index
            .index_chunk(&IndexableChunk {
                id: "chunk-b".to_string(),
                agent_id: "agent-b".to_string(),
                content: "shared keyword".to_string(),
                tags: vec![],
                created_at: 1,
            })
            .unwrap();

        let hits_a = index.search("shared", "agent-a", 10).unwrap();
        assert_eq!(hits_a.len(), 1);
        assert_eq!(hits_a[0].chunk_id, "chunk-a");

        let hits_b = index.search("shared", "agent-b", 10).unwrap();
        assert_eq!(hits_b.len(), 1);
        assert_eq!(hits_b[0].chunk_id, "chunk-b");
    }

    #[test]
    fn test_remove_chunk() {
        let index = MemoryIndex::in_memory().unwrap();

        index
            .index_chunk(&IndexableChunk {
                id: "chunk-1".to_string(),
                agent_id: "agent-a".to_string(),
                content: "content to delete".to_string(),
                tags: vec![],
                created_at: 1,
            })
            .unwrap();

        assert_eq!(index.search("delete", "agent-a", 10).unwrap().len(), 1);
        index.remove_chunk("chunk-1").unwrap();
        assert!(index.search("delete", "agent-a", 10).unwrap().is_empty());
    }

    #[test]
    fn test_rebuild() {
        let tmp = tempdir().unwrap();
        let index = MemoryIndex::open(tmp.path()).unwrap();

        let rebuilt = index
            .rebuild(vec![
                IndexableChunk {
                    id: "chunk-1".to_string(),
                    agent_id: "agent-a".to_string(),
                    content: "hello world".to_string(),
                    tags: vec!["hello".to_string()],
                    created_at: 1,
                },
                IndexableChunk {
                    id: "chunk-2".to_string(),
                    agent_id: "agent-a".to_string(),
                    content: "rust world".to_string(),
                    tags: vec!["rust".to_string()],
                    created_at: 2,
                },
            ])
            .unwrap();

        assert_eq!(rebuilt, 2);
        assert_eq!(index.doc_count().unwrap(), 2);

        let hits = index.search("rust", "agent-a", 10).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].chunk_id, "chunk-2");
    }
}
