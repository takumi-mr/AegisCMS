import type { Meta, StoryObj } from '@storybook/react-vite';
import { fn } from 'storybook/test';
import { RegisterForm } from './RegisterForm';

const meta = {
  title: 'Authentication/RegisterForm',
  component: RegisterForm,
  args: { onBack: fn() },
} satisfies Meta<typeof RegisterForm>;
export default meta;
type Story = StoryObj<typeof meta>;
export const Default: Story = {};
