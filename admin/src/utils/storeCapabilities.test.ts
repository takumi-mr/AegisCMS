import { describe, expect, it } from 'vitest';
import { storeCapabilities } from './storeCapabilities';

describe('storeCapabilities', () => {
  it('lists what a plugin is allowed to do', () => {
    expect(
      storeCapabilities({
        hooks: ['on-render-page', 'render-blocks'],
        storage: true,
        network: { allowed_domains: ['api.example.com'] },
        embed: { frame_domains: ['www.youtube-nocookie.com'], site_domains: true },
        inline_scripts: true,
        render_page: { input: ['fields'] },
        csp: { script_domains: ['cdn.example.com'], img_domains: [] },
      })
    ).toEqual([
      { kind: 'hooks', values: ['on-render-page', 'render-blocks'] },
      { kind: 'storage' },
      { kind: 'network', values: ['api.example.com'] },
      { kind: 'frames', values: ['www.youtube-nocookie.com'] },
      { kind: 'siteFrames' },
      { kind: 'scripts', inline: true, local: false },
      { kind: 'csp', directive: 'script-src', values: ['cdn.example.com'] },
    ]);
  });

  it('lists the hosts a theme adds to the CSP', () => {
    expect(
      storeCapabilities({
        csp: { style_domains: ['fonts.googleapis.com'], font_domains: ['fonts.gstatic.com'] },
      })
    ).toEqual([
      { kind: 'csp', directive: 'style-src', values: ['fonts.googleapis.com'] },
      { kind: 'csp', directive: 'font-src', values: ['fonts.gstatic.com'] },
    ]);
  });

  it('ignores empty, unknown and malformed values', () => {
    expect(storeCapabilities({})).toEqual([]);
    expect(
      storeCapabilities({
        hooks: 'on-render-page',
        storage: 'yes',
        network: ['a.example'],
        csp: { script_domains: [1, 'ok.example'], unknown: ['x.example'] },
      })
    ).toEqual([{ kind: 'csp', directive: 'script-src', values: ['ok.example'] }]);
  });
});
