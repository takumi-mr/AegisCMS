import React, { useState } from 'react';
import type { Meta, StoryObj } from '@storybook/react-vite';
import { expect, fn, userEvent, within } from 'storybook/test';
import type { BlockNode, PluginBlockDefinition } from '../types';
import { PluginBlockEditor } from './PluginBlockEditor';

const mockDefinition: PluginBlockDefinition = {
  name: 'official-blocks/hero',
  plugin: 'official-blocks',
  plugin_display_name: 'Official Blocks',
  label: 'Hero Banner',
  description: 'Hero section with title, subtitle, link, and background configuration.',
  fields: [
    {
      name: 'title',
      type: 'string',
      label: 'Title',
      required: true,
      placeholder: 'Enter hero title...',
    },
    {
      name: 'subtitle',
      type: 'text',
      label: 'Subtitle',
      placeholder: 'Enter detailed description...',
    },
    {
      name: 'action_url',
      type: 'url',
      label: 'Action URL',
      placeholder: 'https://example.com/start',
    },
    {
      name: 'align',
      type: 'select',
      label: 'Alignment',
      options: [
        { value: 'left', label: 'Left' },
        { value: 'center', label: 'Center' },
        { value: 'right', label: 'Right' },
      ],
      default: 'center',
    },
    {
      name: 'overlay_opacity',
      type: 'number',
      label: 'Overlay Opacity',
      placeholder: '50',
      help: 'Opacity percentage (0 to 100)',
    },
    {
      name: 'full_height',
      type: 'boolean',
      label: 'Full Height',
      default: false,
    },
    {
      name: 'image_url',
      type: 'image',
      label: 'Background Image',
      placeholder: 'https://example.com/banner.jpg',
    },
  ],
};

const mockBlock: BlockNode = {
  name: 'official-blocks/hero',
  attributes: {
    title: 'Welcome to AegisCMS',
    subtitle: 'The modern and secure content management platform.',
    action_url: 'https://aegiscms.dev',
    align: 'center',
    overlay_opacity: 60,
    full_height: true,
    image_url: 'https://images.unsplash.com/photo-1579546929518-9e396f3cc809?w=600',
  },
};

const PluginBlockEditorWrapper: React.FC<React.ComponentProps<typeof PluginBlockEditor>> = (
  args
) => {
  const [currentBlock, setCurrentBlock] = useState(args.block);
  return (
    <PluginBlockEditor
      {...args}
      block={currentBlock}
      onChange={(updated) => {
        setCurrentBlock(updated);
        args.onChange(updated);
      }}
    />
  );
};

const meta = {
  title: 'Content/PluginBlockEditor',
  component: PluginBlockEditor,
  parameters: { layout: 'padded' },
  args: {
    block: mockBlock,
    definition: mockDefinition,
    canUpload: true,
    onChange: fn(),
  },
  render: (args) => <PluginBlockEditorWrapper {...args} />,
} satisfies Meta<typeof PluginBlockEditor>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {};

export const Empty: Story = {
  args: {
    block: {
      name: 'official-blocks/hero',
      attributes: {},
    },
  },
};

export const WithoutUploadPermission: Story = {
  args: {
    canUpload: false,
  },
};

export const Interactions: Story = {
  play: async ({ canvasElement, args }) => {
    const canvas = within(canvasElement);

    // Verify title is rendered
    const titleInput = canvas.getByDisplayValue('Welcome to AegisCMS');
    await expect(titleInput).toBeInTheDocument();

    // Type in title
    await userEvent.clear(titleInput);
    await userEvent.type(titleInput, 'New Heading');
    await expect(args.onChange).toHaveBeenCalled();

    // Type in subtitle
    const subtitleInput = canvas.getByDisplayValue(
      'The modern and secure content management platform.'
    );
    await userEvent.clear(subtitleInput);
    await userEvent.type(subtitleInput, 'Updated description text');
    await expect(args.onChange).toHaveBeenCalled();

    // Toggle full_height boolean checkbox
    const checkbox = canvas.getByRole('checkbox', { name: /Full Height/i });
    await expect(checkbox).toBeChecked();
    await userEvent.click(checkbox);
    await expect(args.onChange).toHaveBeenCalled();

    // Change alignment select
    const select = canvas.getByRole('combobox', { name: /Alignment/i });
    await userEvent.selectOptions(select, 'left');
    await expect(args.onChange).toHaveBeenCalled();
  },
};
