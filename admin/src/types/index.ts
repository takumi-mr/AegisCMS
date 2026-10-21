export interface BlockNode {
  name: string;
  attributes: Record<string, any>;
  children?: BlockNode[];
  text?: string;
}

export interface FieldDefinition {
  name: string;
  type: string;
  required: boolean;
  nullable: boolean;
  unique: boolean;
  index: boolean;
  max_length?: number;
  target?: string;
  indexed_paths: string[];
}

export interface SchemaDefinition {
  content_type: string;
  label: string;
  description?: string;
  fields: FieldDefinition[];
}

export interface ContentItem {
  id: string;
  content_type: string;
  title: string;
  slug: string;
  status: 'draft' | 'published' | 'archived';
  blocks: BlockNode[];
  rendered_html: string;
  author_id?: string;
  published_at?: string;
  created_at: string;
  updated_at: string;
}

export interface CreateContentPayload {
  title: string;
  slug: string;
  status: string;
  blocks: BlockNode[];
  author_id?: string;
}
