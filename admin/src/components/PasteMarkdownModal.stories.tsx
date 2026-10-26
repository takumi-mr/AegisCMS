import type { Meta, StoryObj } from '@storybook/react-vite';
import { fn } from 'storybook/test';
import { PasteMarkdownModal } from './PasteMarkdownModal';

const meta = {
  title: 'Content/PasteMarkdownModal',
  component: PasteMarkdownModal,
  args: { onInsert: fn(), onClose: fn() },
} satisfies Meta<typeof PasteMarkdownModal>;
export default meta;
type Story = StoryObj<typeof meta>;
export const Default: Story = {};
