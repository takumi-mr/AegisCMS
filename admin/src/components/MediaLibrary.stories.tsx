import type { Meta, StoryObj } from '@storybook/react-vite';
import { fn } from 'storybook/test';
import { mockApi, type MockHandler } from '../../.storybook/mockApi';
import type { StorageObject, StorageObjectPage } from '../types';
import { MediaLibrary } from './MediaLibrary';

const image = (name: string, size: number): StorageObject => ({
  key: `images/${name}`,
  url: `/uploads/images/${name}`,
  size,
  content_type: 'image/png',
  last_modified: '2026-11-10T12:00:00Z',
});

const handler: MockHandler = (req) => {
  if (req.path !== '/storage/objects') return undefined;
  const objects = [
    image('hero.png', 184_320),
    image('logo.png', 12_288),
    image('team.png', 640_000),
  ];
  return {
    objects,
    total: objects.length,
    limit: 24,
    offset: 0,
    truncated: false,
  } satisfies StorageObjectPage;
};

const meta = {
  beforeEach: () => mockApi(handler),
  title: 'Media/MediaLibrary',
  component: MediaLibrary,
  args: { onSelect: fn(), onClose: fn(), canDelete: false },
} satisfies Meta<typeof MediaLibrary>;
export default meta;
type Story = StoryObj<typeof meta>;
export const SelectMedia: Story = {};
export const CanDelete: Story = { args: { canDelete: true } };
