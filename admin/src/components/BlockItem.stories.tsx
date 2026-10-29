import React, { useState } from 'react';
import type { Meta, StoryObj } from '@storybook/react-vite';
import { expect, fn, userEvent, within } from 'storybook/test';
import type { BlockNode } from '../types';
import { BlockItem } from './BlockItem';

const mockHtmlBlock: BlockNode = {
  name: 'core/html',
  attributes: {},
  text: '<section class="hero">\n  <h1>Summer Promotion</h1>\n  <a href="/shop" class="btn">Shop Now</a>\n</section>',
};

const mockParagraphBlock: BlockNode = {
  name: 'core/paragraph',
  attributes: {},
  text: 'This is a standard paragraph block explaining product details.',
};

const mockHeadingBlock: BlockNode = {
  name: 'core/heading',
  attributes: { level: 2 },
  text: 'Features and Benefits',
};

const BlockItemWrapper: React.FC<React.ComponentProps<typeof BlockItem>> = (args) => {
  const [currentBlock, setCurrentBlock] = useState(args.block);
  return (
    <BlockItem
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
  title: 'Content/BlockItem',
  component: BlockItem,
  parameters: { layout: 'centered' },
  args: {
    index: 0,
    total: 3,
    canUpload: true,
    onChange: fn(),
    onMoveUp: fn(),
    onMoveDown: fn(),
    onDelete: fn(),
  },
  render: (args) => <BlockItemWrapper {...args} />,
} satisfies Meta<typeof BlockItem>;

export default meta;
type Story = StoryObj<typeof meta>;

export const CustomHtml: Story = {
  args: {
    block: mockHtmlBlock,
  },
  play: async ({ canvasElement, args }) => {
    const canvas = within(canvasElement);

    // カスタムHTMLブロックのヘッダーバッジまたはラベルが表示されていること
    await expect(canvas.getByText('core/html')).toBeVisible();

    // HTMLコードエリアが表示され、初期テキストが含まれていること
    const textarea = canvas.getByRole('textbox');
    await expect(textarea).toBeVisible();
    await expect(textarea).toHaveValue(mockHtmlBlock.text);

    // テキストを変更した際に onChange が呼ばれること
    await userEvent.clear(textarea);
    await userEvent.type(textarea, '<p>Updated Custom Content</p>');
    await expect(args.onChange).toHaveBeenCalledWith(
      expect.objectContaining({
        name: 'core/html',
        text: '<p>Updated Custom Content</p>',
      })
    );

    // 実行モードを sandbox に切り替えた際に attributes.mode が更新されること
    const modeSelect = canvas.getByRole('combobox');
    await expect(modeSelect).toHaveValue('sanitized');
    await userEvent.selectOptions(modeSelect, 'sandbox');
    await expect(args.onChange).toHaveBeenLastCalledWith(
      expect.objectContaining({
        name: 'core/html',
        attributes: { mode: 'sandbox' },
      })
    );
  },
};

export const Paragraph: Story = {
  args: {
    block: mockParagraphBlock,
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const textarea = canvas.getByRole('textbox');
    await expect(textarea).toBeVisible();
    await expect(textarea).toHaveValue(mockParagraphBlock.text);
  },
};

export const Heading: Story = {
  args: {
    block: mockHeadingBlock,
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const input = canvas.getByRole('textbox');
    await expect(input).toBeVisible();
    await expect(input).toHaveValue(mockHeadingBlock.text);
  },
};

export const Actions: Story = {
  args: {
    block: mockHtmlBlock,
    index: 1,
    total: 3,
  },
  play: async ({ canvasElement, args }) => {
    const canvas = within(canvasElement);

    // Move Up ボタンクリック
    const upBtn = canvas.getByTitle(/上へ|move up/i);
    await userEvent.click(upBtn);
    await expect(args.onMoveUp).toHaveBeenCalled();

    // Move Down ボタンクリック
    const downBtn = canvas.getByTitle(/下へ|move down/i);
    await userEvent.click(downBtn);
    await expect(args.onMoveDown).toHaveBeenCalled();

    // Delete ボタンクリック
    const deleteBtn = canvas.getByTitle(/削除|delete/i);
    await userEvent.click(deleteBtn);
    await expect(args.onDelete).toHaveBeenCalled();
  },
};

export const TableBlock: Story = {
  args: {
    block: {
      name: 'core/table',
      attributes: {
        header: true,
        align: [null, null],
        rows: [
          ['Plan', 'Price'],
          ['Basic', '¥1,000'],
        ],
      },
    },
  },
  play: async ({ canvasElement, args }) => {
    const canvas = within(canvasElement);

    // 見出し行とセルが表示されていること
    await expect(canvas.getByText('core/table')).toBeVisible();
    await expect(canvas.getByDisplayValue('Plan')).toBeVisible();

    // セルを編集すると rows が変わること
    const cell = canvas.getByDisplayValue('Basic');
    await userEvent.clear(cell);
    await userEvent.type(cell, 'Pro');
    await expect(args.onChange).toHaveBeenLastCalledWith(
      expect.objectContaining({
        attributes: expect.objectContaining({
          rows: [
            ['Plan', 'Price'],
            ['Pro', '¥1,000'],
          ],
        }),
      })
    );

    // 列を足すと全行に空のセルが増え、揃えの指定も増えること
    await userEvent.click(canvas.getAllByRole('button', { name: /Column|列/ })[0]);
    await expect(args.onChange).toHaveBeenLastCalledWith(
      expect.objectContaining({
        attributes: expect.objectContaining({
          align: [null, null, null],
          rows: [
            ['Plan', 'Price', ''],
            ['Pro', '¥1,000', ''],
          ],
        }),
      })
    );

    // 2 列目を右揃えにできること
    await userEvent.selectOptions(canvas.getAllByRole('combobox')[1], 'right');
    await expect(args.onChange).toHaveBeenLastCalledWith(
      expect.objectContaining({
        attributes: expect.objectContaining({ align: [null, 'right', null] }),
      })
    );
  },
};

export const CalloutBlock: Story = {
  args: {
    block: {
      name: 'core/callout',
      attributes: { variant: 'note' },
      text: 'Back up the database first.',
    },
  },
  play: async ({ canvasElement, args }) => {
    const canvas = within(canvasElement);

    await expect(canvas.getByText('core/callout')).toBeVisible();
    await expect(canvas.getByDisplayValue('Back up the database first.')).toBeVisible();

    // 種類を「警告」に変えられること
    await userEvent.selectOptions(canvas.getByRole('combobox'), 'warning');
    await expect(args.onChange).toHaveBeenLastCalledWith(
      expect.objectContaining({
        name: 'core/callout',
        attributes: expect.objectContaining({ variant: 'warning' }),
      })
    );
  },
};
