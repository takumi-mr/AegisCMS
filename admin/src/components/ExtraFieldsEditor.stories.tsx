import React, { useState } from 'react';
import type { Meta, StoryObj } from '@storybook/react-vite';
import { expect, userEvent, within } from 'storybook/test';
import type { FieldDefinition } from '../types';
import { fromExtraValues, toExtraValues, type ExtraValues } from '../utils/extraFields';
import { ExtraFieldsEditor } from './ExtraFieldsEditor';

const def = (
  name: string,
  type: string,
  extra: Partial<FieldDefinition> = {}
): FieldDefinition => ({
  name,
  type,
  required: false,
  nullable: true,
  unique: false,
  index: false,
  indexed_paths: [],
  ...extra,
});

const defs: FieldDefinition[] = [
  def('venue', 'string'),
  def('capacity', 'integer'),
  def('starts_at', 'timestamp'),
  def('featured', 'boolean', { nullable: false }),
  def('custom_meta', 'jsonb'),
];

const Harness: React.FC = () => {
  const [values, setValues] = useState<ExtraValues>(() =>
    toExtraValues(defs, { venue: 'Hall A', capacity: 120 })
  );
  const result = fromExtraValues(defs, values);
  return (
    <div style={{ display: 'grid', gap: 8, maxWidth: 420 }}>
      <ExtraFieldsEditor
        defs={defs}
        values={values}
        error={'error' in result ? result.error : null}
        onChange={(name, value) => setValues((prev) => ({ ...prev, [name]: value }))}
      />
      <output data-testid="saved">
        {'fields' in result ? JSON.stringify(result.fields) : 'error'}
      </output>
    </div>
  );
};

const meta: Meta<typeof Harness> = {
  title: 'Components/ExtraFieldsEditor',
  component: Harness,
};
export default meta;

type Story = StoryObj<typeof Harness>;

export const EditsValuesByFieldType: Story = {
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);

    const capacity = canvas.getByLabelText(/capacity/);
    await userEvent.clear(capacity);
    await userEvent.type(capacity, '200');
    await userEvent.click(canvas.getByLabelText(/featured/));

    const saved = canvas.getByTestId('saved');
    expect(JSON.parse(saved.textContent ?? '{}')).toMatchObject({
      venue: 'Hall A',
      capacity: 200,
      featured: true,
    });
  },
};

export const RejectsInvalidInput: Story = {
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);

    const capacity = canvas.getByLabelText(/capacity/);
    await userEvent.clear(capacity);
    await userEvent.type(capacity, '1.5');

    expect(await canvas.findByRole('alert')).toHaveTextContent('capacity');
    expect(canvas.getByTestId('saved')).toHaveTextContent('error');
  },
};
