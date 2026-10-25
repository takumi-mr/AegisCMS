import type { Meta, StoryObj } from '@storybook/react-vite';
import { Info } from 'lucide-react';
import { expect, fn, userEvent, within } from 'storybook/test';
import { Modal } from './Modal';
import { Button } from './Button';

const meta = {
  title: 'UI/Modal',
  component: Modal,
  parameters: { layout: 'fullscreen' },
  args: {
    title: 'Confirm Operation',
    icon: <Info size={18} />,
    onClose: fn(),
    children: (
      <div>
        <p>Are you sure you want to proceed with this operation?</p>
        <div
          style={{
            marginTop: '1.5rem',
            display: 'flex',
            justifyContent: 'flex-end',
            gap: '0.5rem',
          }}
        >
          <Button variant="secondary">Cancel</Button>
          <Button variant="primary">Proceed</Button>
        </div>
      </div>
    ),
  },
} satisfies Meta<typeof Modal>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  play: async ({ canvasElement, args }) => {
    const canvas = within(canvasElement);
    await expect(canvas.getByRole('dialog')).toBeVisible();
    await expect(canvas.getByText('Confirm Operation')).toBeVisible();

    const closeBtn = canvas.getByRole('button', { name: 'Close' });
    await userEvent.click(closeBtn);
    await expect(args.onClose).toHaveBeenCalled();
  },
};

export const EscapeKeyCloses: Story = {
  play: async ({ canvasElement, args }) => {
    const canvas = within(canvasElement);
    await expect(canvas.getByRole('dialog')).toBeVisible();

    await userEvent.keyboard('{Escape}');
    await expect(args.onClose).toHaveBeenCalled();
  },
};

export const Wide: Story = {
  args: {
    wide: true,
    title: 'Wide Information Modal',
  },
};
