use std::path::Path;

use memory_core::{Node, NodeId};
use memory_store::StoreRepository;
use tantivy::collector::TopDocs;
use tantivy::directory::RamDirectory;
use tantivy::query::QueryParser;
use tantivy::schema::{Field, Schema, STORED, STRING, TEXT, Value as _};
use tantivy::{doc, Index, IndexReader, IndexWriter, Term};
use tracing::debug;

use crate::RetrievalError;

#[derive(Debug, Clone, PartialEq)]
pub struct LexicalCandidate {
    pub node_id: NodeId,
    pub lexical_score: f32,
    pub matched_fields: Vec<String>,
}

pub trait LexicalSearch {
    fn search(&self, query: &str, limit: usize) -> Result<Vec<LexicalCandidate>, RetrievalError>;
}

pub trait LexicalIndexWriter {
    fn upsert_node(&mut self, node: &Node) -> Result<(), RetrievalError>;
}

#[derive(Clone)]
pub struct TantivyLexicalIndex {
    index: Index,
    schema: LexicalSchema,
    reader: IndexReader,
}

#[derive(Debug, Clone, Copy)]
struct LexicalSchema {
    node_id: Field,
    node_type: Field,
    title: Field,
    summary: Field,
    content: Field,
    tags: Field,
}

impl TantivyLexicalIndex {
    pub fn from_nodes(nodes: &[Node]) -> Result<Self, RetrievalError> {
        let index = Self::in_memory()?;
        let mut writable = index.writer(15_000_000)?;
        for node in nodes {
            writable.upsert_node(node)?;
        }
        Ok(index)
    }

    pub fn in_memory() -> Result<Self, RetrievalError> {
        let built = build_schema();
        let index = Index::open_or_create(RamDirectory::default(), built.schema.clone())?;
        let reader = index.reader()?;
        Ok(Self {
            index,
            schema: built.fields,
            reader,
        })
    }

    pub fn open_or_create(path: impl AsRef<Path>) -> Result<Self, RetrievalError> {
        let built = build_schema();
        let directory = tantivy::directory::MmapDirectory::open(path)
            .map_err(|error| RetrievalError::Lexical(error.to_string()))?;
        let index = Index::open_or_create(directory, built.schema.clone())?;
        let reader = index.reader()?;
        Ok(Self {
            index,
            schema: built.fields,
            reader,
        })
    }

    pub fn writer(&self, heap_size_bytes: usize) -> Result<TantivyLexicalWriter, RetrievalError> {
        Ok(TantivyLexicalWriter {
            writer: self.index.writer(heap_size_bytes)?,
            schema: self.schema,
            reader: self.reader.clone(),
        })
    }
}

impl LexicalSearch for TantivyLexicalIndex {
    fn search(&self, query: &str, limit: usize) -> Result<Vec<LexicalCandidate>, RetrievalError> {
        if query.trim().is_empty() {
            return Ok(Vec::new());
        }

        let searcher = self.reader.searcher();
        let parser = QueryParser::for_index(
            &self.index,
            vec![
                self.schema.title,
                self.schema.summary,
                self.schema.content,
                self.schema.tags,
                self.schema.node_type,
            ],
        );
        let parsed_query = parser
            .parse_query(query)
            .map_err(|error| RetrievalError::Lexical(error.to_string()))?;
        let docs = searcher.search(&parsed_query, &TopDocs::with_limit(limit))?;
        let query_terms = query
            .split_whitespace()
            .map(|term| term.trim_matches(|character: char| !character.is_alphanumeric()))
            .filter(|term| !term.is_empty())
            .map(str::to_ascii_lowercase)
            .collect::<Vec<_>>();

        let mut hits = Vec::new();
        for (score, address) in docs {
            let retrieved = searcher.doc::<tantivy::schema::TantivyDocument>(address)?;
            let Some(node_id) = field_text(&retrieved, self.schema.node_id)
                .and_then(|value| uuid::Uuid::parse_str(&value).ok())
                .map(NodeId)
            else {
                continue;
            };

            let matched_fields = matched_fields(&retrieved, &self.schema, &query_terms);
            hits.push(LexicalCandidate {
                node_id,
                lexical_score: score,
                matched_fields,
            });
        }

        debug!(query = %query, hits = hits.len(), "tantivy lexical hits collected");
        Ok(hits)
    }
}

pub struct TantivyLexicalWriter {
    writer: IndexWriter,
    schema: LexicalSchema,
    reader: IndexReader,
}

impl LexicalIndexWriter for TantivyLexicalWriter {
    fn upsert_node(&mut self, node: &Node) -> Result<(), RetrievalError> {
        index_document(&mut self.writer, &self.schema, node);
        self.writer.commit()?;
        self.reader.reload()?;
        Ok(())
    }
}

impl TantivyLexicalWriter {
    pub fn delete_node(&mut self, node_id: NodeId) -> Result<(), RetrievalError> {
        self.writer
            .delete_term(Term::from_field_text(self.schema.node_id, &node_id.0.to_string()));
        self.writer.commit()?;
        self.reader.reload()?;
        Ok(())
    }
}

/// Optional wrapper that keeps Tantivy in sync when nodes are inserted or updated in the store.
///
/// The libSQL/Turso store remains the source of truth. This wrapper is a maintenance hook for the
/// local lexical index and can be used by the application or memory engine write path.
pub struct IndexedStoreRepository<'a, I> {
    repository: StoreRepository<'a>,
    lexical_index: I,
}

impl<'a, I> IndexedStoreRepository<'a, I>
where
    I: LexicalIndexWriter,
{
    pub fn new(repository: StoreRepository<'a>, lexical_index: I) -> Self {
        Self {
            repository,
            lexical_index,
        }
    }

    pub async fn insert_node(&mut self, node: &Node) -> Result<Node, RetrievalError> {
        let saved = self.repository.insert_node(node).await?;
        self.lexical_index.upsert_node(&saved)?;
        Ok(saved)
    }

    pub async fn update_node(&mut self, node: &Node) -> Result<Option<Node>, RetrievalError> {
        let updated = self.repository.update_node(node).await?;
        if let Some(saved) = &updated {
            self.lexical_index.upsert_node(saved)?;
        }
        Ok(updated)
    }
}

#[derive(Debug, Clone)]
struct BuiltSchema {
    schema: Schema,
    fields: LexicalSchema,
}

fn build_schema() -> BuiltSchema {
    let mut schema_builder = Schema::builder();
    let node_id = schema_builder.add_text_field("node_id", STRING | STORED);
    let node_type = schema_builder.add_text_field("node_type", STRING | STORED);
    let title = schema_builder.add_text_field("title", TEXT | STORED);
    let summary = schema_builder.add_text_field("summary", TEXT | STORED);
    let content = schema_builder.add_text_field("content", TEXT | STORED);
    let tags = schema_builder.add_text_field("tags", TEXT | STORED);
    let schema = schema_builder.build();

    BuiltSchema {
        schema,
        fields: LexicalSchema {
            node_id,
            node_type,
            title,
            summary,
            content,
            tags,
        },
    }
}

fn index_document(writer: &mut IndexWriter, schema: &LexicalSchema, node: &Node) {
    writer.delete_term(Term::from_field_text(schema.node_id, &node.id.0.to_string()));
    let _ = writer.add_document(doc!(
        schema.node_id => node.id.0.to_string(),
        schema.node_type => format!("{:?}", node.node_type).to_ascii_lowercase(),
        schema.title => node.title.clone(),
        schema.summary => node.summary.clone(),
        schema.content => node.content.clone().unwrap_or_default(),
        schema.tags => node.tags.join(" "),
    ));
}

fn field_text(document: &tantivy::schema::TantivyDocument, field: Field) -> Option<String> {
    document
        .get_first(field)
        .and_then(|value| value.as_str())
        .map(str::to_owned)
}

fn matched_fields(
    document: &tantivy::schema::TantivyDocument,
    schema: &LexicalSchema,
    query_terms: &[String],
) -> Vec<String> {
    let mut matched = Vec::new();

    for (name, field) in [
        ("title", schema.title),
        ("summary", schema.summary),
        ("content", schema.content),
        ("tags", schema.tags),
    ] {
        let field_value = field_text(document, field)
            .unwrap_or_default()
            .to_ascii_lowercase();
        if query_terms.iter().any(|term| field_value.contains(term)) {
            matched.push(name.to_owned());
        }
    }

    matched
}
