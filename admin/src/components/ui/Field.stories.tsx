import type { Meta, StoryObj } from '@storybook/react-vite';
import { expect, userEvent, within } from 'storybook/test';
import { Field, FieldRow } from './Field';

const meta = {
  title: 'UI/Field',
  component: Field,
  args: { label: 'Slug', children: <input aria-label="Slug" placeholder="post-slug" /> },
  render: (args) => (
    <FieldRow>
      <Field {...args} as="label">
        <input aria-label="Slug" placeholder="post-slug" />
      </Field>
    </FieldRow>
  ),
} satisfies Meta<typeof Field>;

export default meta;
type Story = StoryObj<typeof meta>;

export const TextInput: Story = {
  args: { label: 'Slug', children: <input aria-label="Slug" placeholder="post-slug" /> },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await userEvent.click(canvas.getByText('Slug'));
    await expect(document.activeElement).toBe(canvas.getByRole('textbox', { name: 'Slug' }));
  },
};

export const SelectInput: Story = {
  args: { label: 'Status', children: <select aria-label="Status" defaultValue="draft" /> },
  render: () => (
    <FieldRow>
      <Field label="Status" as="label">
        <select aria-label="Status" defaultValue="draft">
          <option value="draft">Draft</option>
          <option value="published">Published</option>
        </select>
      </Field>
    </FieldRow>
  ),
  play: async ({ canvasElement }) => {
    const status = within(canvasElement).getByRole('combobox', { name: 'Status' });
    await userEvent.selectOptions(status, 'published');
    await expect(status).toHaveValue('published');
  },
};
