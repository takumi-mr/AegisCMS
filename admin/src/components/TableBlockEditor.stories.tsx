import React, { useState } from 'react';
import type { Meta, StoryObj } from '@storybook/react-vite';
import { expect, fn, userEvent, within } from 'storybook/test';
import type { BlockNode } from '../types';
import { TableBlockEditor } from './TableBlockEditor';

const mockTableBlock: BlockNode = {
  name: 'core/table',
  attributes: {
    header: true,
    align: ['left', 'center', 'right'],
    rows: [
      ['Item', 'Status', 'Count'],
      ['Alpha', 'Active', '42'],
      ['Beta', 'Pending', '7'],
    ],
  },
};

const TableBlockEditorWrapper: React.FC<React.ComponentProps<typeof TableBlockEditor>> = (args) => {
  const [currentBlock, setCurrentBlock] = useState(args.block);
  return (
    <TableBlockEditor
      {...args}
      block={currentBlock}
      onChange={(updated) => {
        setCurrentBlock(updated);
        args.onChange?.(updated);
      }}
    />
  );
};

const meta = {
  title: 'Content/TableBlockEditor',
  component: TableBlockEditor,
  parameters: { layout: 'padded' },
  args: {
    block: mockTableBlock,
    onChange: fn(),
  },
  render: (args) => <TableBlockEditorWrapper {...args} />,
} satisfies Meta<typeof TableBlockEditor>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {};

export const WithoutHeader: Story = {
  args: {
    block: {
      name: 'core/table',
      attributes: {
        header: false,
        align: ['left', 'left'],
        rows: [
          ['Row 1 Col 1', 'Row 1 Col 2'],
          ['Row 2 Col 1', 'Row 2 Col 2'],
        ],
      },
    },
  },
};

export const Empty: Story = {
  args: {
    block: {
      name: 'core/table',
      attributes: {
        header: true,
        align: [null],
        rows: [['']],
      },
    },
  },
};

export const Interactions: Story = {
  play: async ({ canvasElement, args }) => {
    const canvas = within(canvasElement);

    // Verify initial cells exist
    const firstCell = canvas.getByDisplayValue('Item');
    await expect(firstCell).toBeInTheDocument();

    // Edit cell
    await userEvent.clear(firstCell);
    await userEvent.type(firstCell, 'Component');
    await expect(args.onChange).toHaveBeenCalled();

    // Add row
    const addRowBtn = canvas.getAllByRole('button', { name: /Row/i })[0];
    await userEvent.click(addRowBtn);
    await expect(args.onChange).toHaveBeenCalled();

    // Add column
    const addColBtn = canvas.getAllByRole('button', { name: /Column/i })[0];
    await userEvent.click(addColBtn);
    await expect(args.onChange).toHaveBeenCalled();

    // Toggle header checkbox
    const headerCheckbox = canvas.getByRole('checkbox');
    await userEvent.click(headerCheckbox);
    await expect(args.onChange).toHaveBeenCalled();
  },
};
