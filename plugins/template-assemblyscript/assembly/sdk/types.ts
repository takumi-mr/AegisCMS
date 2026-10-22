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

export class PageRenderContext {
  site_title: string = "";
  page_title: string = "";
  slug: string = "";
  template_name: string = "article.html";
  extra_css: string = "";
  extra_head: string = "";
  variables: Map<string, string> = new Map<string, string>();
}
