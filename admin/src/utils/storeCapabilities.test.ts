import { describe, expect, it } from 'vitest';
import { capabilityAdditions, storeCapabilities } from './storeCapabilities';

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

describe('capabilityAdditions', () => {
  it('lists only what the new version adds', () => {
    expect(
      capabilityAdditions(
        {
          hooks: ['on-render-page', 'render-blocks'],
          storage: true,
          inline_scripts: true,
          local_scripts: true,
          network: { allowed_domains: ['a.example', 'b.example'] },
          csp: { script_domains: ['cdn.example'], img_domains: ['img.example'] },
        },
        {
          hooks: ['on-render-page'],
          storage: true,
          inline_scripts: true,
          network: { allowed_domains: ['a.example'] },
          csp: { script_domains: ['cdn.example'] },
        }
      )
    ).toEqual([
      { kind: 'hooks', values: ['render-blocks'] },
      { kind: 'network', values: ['b.example'] },
      { kind: 'scripts', inline: false, local: true },
      { kind: 'csp', directive: 'img-src', values: ['img.example'] },
    ]);
  });

  it('is empty when nothing new is allowed, and lists everything for a first install', () => {
    const caps = { hooks: ['on-render-page'], csp: { font_domains: ['fonts.example'] } };
    expect(capabilityAdditions(caps, caps)).toEqual([]);
    expect(capabilityAdditions({ hooks: ['on-render-page'] }, caps)).toEqual([]);
    expect(capabilityAdditions(caps, {})).toEqual(storeCapabilities(caps));
  });
});
