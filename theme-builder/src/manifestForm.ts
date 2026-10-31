export interface ManifestFormData {
  id: string;
  name: string;
  version: string;
  description: string;
  authorName: string;
  authorEmail: string;
  authorUrl: string;
  license: string;
  previewImage: string;
  tags: string;
  homepage: string;
  repository: string;
  minAegisVersion: string;
}

export function parseManifestJson(
  jsonText: string
): { ok: true; data: ManifestFormData; raw: unknown } | { ok: false; error: string } {
  try {
    const obj = JSON.parse(jsonText);
    if (typeof obj !== 'object' || obj === null || Array.isArray(obj)) {
      return { ok: false, error: 'マニフェストは JSON オブジェクトである必要があります' };
    }
    const author = obj.author;
    let authorName = '';
    let authorEmail = '';
    let authorUrl = '';
    if (typeof author === 'string') {
      authorName = author;
    } else if (typeof author === 'object' && author !== null) {
      authorName = typeof author.name === 'string' ? author.name : '';
      authorEmail = typeof author.email === 'string' ? author.email : '';
      authorUrl = typeof author.url === 'string' ? author.url : '';
    }

    const tags = Array.isArray(obj.tags)
      ? obj.tags.filter((t: unknown) => typeof t === 'string').join(', ')
      : '';

    return {
      ok: true,
      data: {
        id: typeof obj.id === 'string' ? obj.id : '',
        name: typeof obj.name === 'string' ? obj.name : '',
        version: typeof obj.version === 'string' ? obj.version : '1.0.0',
        description: typeof obj.description === 'string' ? obj.description : '',
        authorName,
        authorEmail,
        authorUrl,
        license: typeof obj.license === 'string' ? obj.license : '',
        previewImage: typeof obj.preview_image === 'string' ? obj.preview_image : '',
        tags,
        homepage: typeof obj.homepage === 'string' ? obj.homepage : '',
        repository: typeof obj.repository === 'string' ? obj.repository : '',
        minAegisVersion: typeof obj.min_aegis_version === 'string' ? obj.min_aegis_version : '',
      },
      raw: obj,
    };
  } catch (e) {
    return { ok: false, error: e instanceof Error ? e.message : 'Invalid JSON' };
  }
}

export function serializeManifestJson(data: ManifestFormData, rawOriginal?: unknown): string {
  const base =
    typeof rawOriginal === 'object' && rawOriginal !== null && !Array.isArray(rawOriginal)
      ? { ...(rawOriginal as Record<string, unknown>) }
      : {};

  base.id = data.id.trim();
  base.name = data.name.trim();
  base.version = data.version.trim() || '1.0.0';
  base.description = data.description;

  // Author
  if (data.authorEmail.trim() || data.authorUrl.trim()) {
    const authorObj: Record<string, string> = { name: data.authorName.trim() };
    if (data.authorEmail.trim()) authorObj.email = data.authorEmail.trim();
    if (data.authorUrl.trim()) authorObj.url = data.authorUrl.trim();
    base.author = authorObj;
  } else if (data.authorName.trim()) {
    if (typeof base.author === 'object' && base.author !== null) {
      base.author = { name: data.authorName.trim() };
    } else {
      base.author = data.authorName.trim();
    }
  }

  // License
  if (data.license.trim()) {
    base.license = data.license.trim();
  } else {
    delete base.license;
  }

  // Preview Image
  if (data.previewImage.trim()) {
    base.preview_image = data.previewImage.trim();
  } else {
    delete base.preview_image;
  }

  // Tags
  const tagList = data.tags
    .split(',')
    .map((t) => t.trim())
    .filter((t) => t.length > 0);
  if (tagList.length > 0) {
    base.tags = tagList;
  } else {
    delete base.tags;
  }

  // Homepage
  if (data.homepage.trim()) {
    base.homepage = data.homepage.trim();
  } else {
    delete base.homepage;
  }

  // Repository
  if (data.repository.trim()) {
    base.repository = data.repository.trim();
  } else {
    delete base.repository;
  }

  // Min Aegis Version
  if (data.minAegisVersion.trim()) {
    base.min_aegis_version = data.minAegisVersion.trim();
  } else {
    delete base.min_aegis_version;
  }

  return JSON.stringify(base, null, 2) + '\n';
}
