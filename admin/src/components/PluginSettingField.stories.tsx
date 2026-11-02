import React, { useState } from 'react';
import type { Meta, StoryObj } from '@storybook/react-vite';
import { expect, fn, userEvent, within } from 'storybook/test';
import type { PluginSettingDefinition } from '../types';
import { PluginSettingField } from './PluginSettingField';

const mockStringSetting: PluginSettingDefinition = {
  key: 'api_endpoint',
  type: 'string',
  label: 'API Endpoint',
  description: 'The remote service base URL for content synchronization.',
  placeholder: 'https://api.example.com',
  default: 'https://api.default.org',
};

const mockTextSetting: PluginSettingDefinition = {
  key: 'custom_header_html',
  type: 'text',
  label: 'Custom Header HTML',
  description: 'Raw HTML injected into the page <head>.',
  placeholder: '<meta name="custom" content="value" />',
};

const mockNumberSetting: PluginSettingDefinition = {
  key: 'max_display_items',
  type: 'number',
  label: 'Max Items',
  description: 'Maximum number of items shown per page.',
  default: 10,
  min: 1,
  max: 100,
};

const mockBooleanSetting: PluginSettingDefinition = {
  key: 'enable_analytics',
  type: 'boolean',
  label: 'Enable Tracking',
  description: 'Whether to collect anonymous performance telemetry.',
  default: 'true',
};

const mockSelectSetting: PluginSettingDefinition = {
  key: 'theme_style',
  type: 'select',
  label: 'Theme Style',
  description: 'Visual accent palette for rendered widgets.',
  default: 'ocean',
  options: [
    { value: 'ocean', label: 'Ocean Blue' },
    { value: 'forest', label: 'Forest Green' },
    { value: 'sunset', label: 'Sunset Orange' },
  ],
};

const mockColorSetting: PluginSettingDefinition = {
  key: 'brand_accent_color',
  type: 'color',
  label: 'Brand Accent Color',
  description: 'Primary theme hex color code.',
  default: '#6366f1',
};

const PluginSettingFieldWrapper: React.FC<React.ComponentProps<typeof PluginSettingField>> = (
  args
) => {
  const [val, setVal] = useState(args.value);
  return (
    <PluginSettingField
      {...args}
      value={val}
      onChange={(next) => {
        setVal(next);
        args.onChange(next);
      }}
    />
  );
};

const meta = {
  title: 'Plugins/PluginSettingField',
  component: PluginSettingField,
  parameters: { layout: 'padded' },
  args: {
    setting: mockStringSetting,
    value: '',
    error: null,
    disabled: false,
    onChange: fn(),
  },
  render: (args) => <PluginSettingFieldWrapper {...args} />,
} satisfies Meta<typeof PluginSettingField>;

export default meta;
type Story = StoryObj<typeof meta>;

export const StringInput: Story = {
  args: {
    setting: mockStringSetting,
    value: 'https://api.mycustomdomain.com',
  },
};

export const MultilineText: Story = {
  args: {
    setting: mockTextSetting,
    value: '<!-- Analytics script -->\n<script async src="/cdn.js"></script>',
  },
};

export const NumberInput: Story = {
  args: {
    setting: mockNumberSetting,
    value: '25',
  },
};

export const BooleanSelect: Story = {
  args: {
    setting: mockBooleanSetting,
    value: 'true',
  },
};

export const SelectOptions: Story = {
  args: {
    setting: mockSelectSetting,
    value: 'forest',
  },
};

export const ColorPicker: Story = {
  args: {
    setting: mockColorSetting,
    value: '#10b981',
  },
};

export const WithError: Story = {
  args: {
    setting: mockStringSetting,
    value: 'invalid-url-format',
    error: 'Must be a valid absolute URL starting with https://',
  },
};

export const Disabled: Story = {
  args: {
    setting: mockStringSetting,
    value: 'https://read-only.example.com',
    disabled: true,
  },
};

export const Interactions: Story = {
  args: {
    setting: mockStringSetting,
    value: '',
  },
  play: async ({ canvasElement, args }) => {
    const canvas = within(canvasElement);

    const input = canvas.getByRole('textbox');
    await userEvent.type(input, 'https://test.example.com');
    await expect(args.onChange).toHaveBeenCalled();
  },
};
