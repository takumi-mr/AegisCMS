import type { Meta, StoryObj } from '@storybook/react-vite';
import { fn } from 'storybook/test';
import { SetupForm } from './SetupForm';

const meta = {
  title: 'Authentication/SetupForm',
  component: SetupForm,
  args: { onSetupComplete: fn(), onBack: fn() },
} satisfies Meta<typeof SetupForm>;
export default meta;
type Story = StoryObj<typeof meta>;
export const Default: Story = {};
