import { describe, expect, it } from 'vitest';
import { isValidNewFilePath } from './newFile.ts';

describe('isValidNewFilePath', () => {
  it('accepts text files under templates/, samples/ and assets/', () => {
    for (const path of [
      'templates/card.html',
      'templates/partials/card.html',
      'samples/wide.json',
      'assets/css/theme-dark.css',
      'assets/app.min.js',
    ]) {
      expect(isValidNewFilePath(path), path).toBe(true);
    }
  });

  it('refuses other places, other types, and ways out of the theme folder', () => {
    for (const path of [
      '',
      'index.html',
      'theme.json',
      'config/default.toml',
      'templates/',
      'templates/a.png',
      'templates/noextension',
      'templates/../theme.json',
      'templates/../x.html',
      'templates/./x.html',
      'templates/.hidden.html',
      'templates//x.html',
      '/templates/x.html',
      'templates\\x.html',
      'templates/a b.html',
    ]) {
      expect(isValidNewFilePath(path), path).toBe(false);
    }
  });
});
