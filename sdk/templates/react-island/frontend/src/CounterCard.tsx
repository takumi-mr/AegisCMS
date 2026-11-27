import { useState } from "react";

export interface CounterCardProps {
  title: string;
  initial: number;
  step: number;
}

export function CounterCard({ title, initial, step }: CounterCardProps) {
  const [count, setCount] = useState(initial);

  return (
    <section className="ri-card">
      <h3 className="ri-title">{title}</h3>
      <div className="ri-controls">
        <button
          type="button"
          className="ri-btn"
          onClick={() => setCount((c) => c - step)}
        >
          -{step}
        </button>
        <span className="ri-value" aria-live="polite">
          {count}
        </span>
        <button
          type="button"
          className="ri-btn ri-btn--primary"
          onClick={() => setCount((c) => c + step)}
        >
          +{step}
        </button>
      </div>
    </section>
  );
}
