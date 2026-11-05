//! 配信 API が DB から読めるものの入口。
//!
//! 公開版の取得だけを持つ小さなトレイトにして、編集用のメソッド（下書きや編集中の内容を返すもの）が
//! このクレートからは型として見えないようにする。下書きを返す処理は、書こうとしても書けない。

use std::collections::HashMap;

use aegis_db::{ContentListQuery, ContentPage, ContentRecord, DatabaseAdapter, ResolvedRelation};
use aegis_schema::SchemaDefinition;
use async_trait::async_trait;

/// DB の失敗（呼び出し側は 500 にする。中身は応答に出さない）
pub type SourceError = aegis_core::AegisError;

#[async_trait]
pub trait PublishedSource: Send + Sync {
    /// 公開版の 1 件（slug または ID）
    async fn get_published(
        &self,
        schema: &SchemaDefinition,
        slug_or_id: &str,
    ) -> Result<Option<ContentRecord>, SourceError>;

    /// 公開版の一覧（作成日の新しい順。キーセット・カテゴリ・タグの絞り込み）
    async fn list_published(
        &self,
        schema: &SchemaDefinition,
        query: &ContentListQuery,
    ) -> Result<ContentPage, SourceError>;

    /// 参照先を、公開中のものだけ引く
    async fn resolve_published_relations(
        &self,
        target_schema: &SchemaDefinition,
        target_field: &str,
        ref_ids: &[String],
    ) -> Result<HashMap<String, ResolvedRelation>, SourceError>;

    /// タグの slug から ID
    async fn tag_id_by_slug(&self, slug: &str) -> Result<Option<String>, SourceError>;
}

#[async_trait]
impl<T: DatabaseAdapter + ?Sized> PublishedSource for T {
    async fn get_published(
        &self,
        schema: &SchemaDefinition,
        slug_or_id: &str,
    ) -> Result<Option<ContentRecord>, SourceError> {
        self.get_published_content(schema, slug_or_id).await
    }

    async fn list_published(
        &self,
        schema: &SchemaDefinition,
        query: &ContentListQuery,
    ) -> Result<ContentPage, SourceError> {
        self.list_published_contents_page(schema, query).await
    }

    async fn resolve_published_relations(
        &self,
        target_schema: &SchemaDefinition,
        target_field: &str,
        ref_ids: &[String],
    ) -> Result<HashMap<String, ResolvedRelation>, SourceError> {
        DatabaseAdapter::resolve_published_relations(self, target_schema, target_field, ref_ids)
            .await
    }

    async fn tag_id_by_slug(&self, slug: &str) -> Result<Option<String>, SourceError> {
        Ok(self.get_tag_by_slug(slug).await?.map(|tag| tag.id))
    }
}
