export * from './schemas';

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

// Backward compatibility alias for generic content lists
export type ContentItem = import('./schemas').BaseContentItem;
export type CreateContentPayload = import('./schemas').CreateArticlePayload;
