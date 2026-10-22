/**
 * AegisCMS AssemblyScript Plugin SDK - Types
 */

export class PostContext {
  id: string = "";
  title: string = "";
  slug: string = "";
  content_type: string = "article";
  status: string = "draft";
  author_id: string = "";
  blocks_json: string = "[]";
  created_at: i64 = 0;
  updated_at: i64 = 0;

  constructor(title: string = "", slug: string = "") {
    this.title = title;
    this.slug = slug;
  }
}

export class BlockNode {
  name: string = "core/paragraph";
  text: string | null = null;
  attributes: Map<string, string> = new Map<string, string>();
  children: Array<BlockNode> = new Array<BlockNode>();

  constructor(name: string = "core/paragraph", text: string | null = null) {
    this.name = name;
    this.text = text;
  }

  static heading(level: i32, text: string): BlockNode {
    const node = new BlockNode("core/heading", text);
    node.attributes.set("level", level.toString());
    return node;
  }

  static paragraph(text: string): BlockNode {
    return new BlockNode("core/paragraph", text);
  }
}

/**
 * `on_render_page` で扱うページ情報（ホストの `PageRenderContext` の文字列フィールド）。
 * ホストは返した JSON のフィールドだけを上書きするため、本文 (`content_html`) や記事一覧
 * (`items`) などここに無いフィールドはそのまま保たれる
 */
export class PageRenderContext {
  site_title: string = "";
  site_description: string = "";
  template: string = "";
  content_type: string = "";
  title: string = "";
  slug: string = "";
  extra_css: string = "";
  extra_head: string = "";
}
