import { describe, expect, it } from 'vitest';
import { parseManifestJson, serializeManifestJson, type ManifestFormData } from './manifestForm.ts';

describe('manifestForm', () => {
  it('parses a full theme.json with object author', () => {
    const json = JSON.stringify({
      id: 'my-theme',
      name: 'My Theme',
      version: '1.2.0',
      description: 'A great theme',
      author: {
        name: 'Alice',
        email: 'alice@example.com',
        url: 'https://example.com',
      },
      license: 'MIT',
      preview_image: 'preview.svg',
      tags: ['dark', 'modern'],
      homepage: 'https://example.com/theme',
      repository: 'https://github.com/example/theme',
      min_aegis_version: '0.1.0',
    });

    const parsed = parseManifestJson(json);
    expect(parsed.ok).toBe(true);
    if (!parsed.ok) return;

    expect(parsed.data.id).toBe('my-theme');
    expect(parsed.data.name).toBe('My Theme');
    expect(parsed.data.version).toBe('1.2.0');
    expect(parsed.data.authorName).toBe('Alice');
    expect(parsed.data.authorEmail).toBe('alice@example.com');
    expect(parsed.data.tags).toBe('dark, modern');
  });

  it('parses a theme.json with string author and minimal fields', () => {
    const json = JSON.stringify({
      id: 'simple',
      name: 'Simple Theme',
      author: 'Bob',
    });

    const parsed = parseManifestJson(json);
    expect(parsed.ok).toBe(true);
    if (!parsed.ok) return;

    expect(parsed.data.id).toBe('simple');
    expect(parsed.data.authorName).toBe('Bob');
    expect(parsed.data.authorEmail).toBe('');
    expect(parsed.data.version).toBe('1.0.0');
  });

  it('returns an error on invalid json', () => {
    const parsed = parseManifestJson('{ invalid');
    expect(parsed.ok).toBe(false);
  });

  it('serializes form data back to formatted JSON preserving unknown fields', () => {
    const original = {
      id: 'old-id',
      name: 'Old Name',
      custom_field: 42,
    };

    const formData: ManifestFormData = {
      id: 'new-id',
      name: 'New Name',
      version: '2.0.0',
      description: 'Updated description',
      authorName: 'Charlie',
      authorEmail: 'charlie@example.com',
      authorUrl: '',
      license: 'Apache-2.0',
      previewImage: 'preview.png',
      tags: 'tech, cool, fast',
      homepage: '',
      repository: 'https://github.com/test/repo',
      minAegisVersion: '0.2.0',
    };

    const serialized = serializeManifestJson(formData, original);
    const parsed = JSON.parse(serialized);

    expect(parsed.id).toBe('new-id');
    expect(parsed.name).toBe('New Name');
    expect(parsed.version).toBe('2.0.0');
    expect(parsed.custom_field).toBe(42);
    expect(parsed.author).toEqual({ name: 'Charlie', email: 'charlie@example.com' });
    expect(parsed.license).toBe('Apache-2.0');
    expect(parsed.tags).toEqual(['tech', 'cool', 'fast']);
    expect(parsed.homepage).toBeUndefined();
    expect(parsed.repository).toBe('https://github.com/test/repo');
  });
});
