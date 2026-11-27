import { createRoot } from "react-dom/client";
import { CounterCard, type CounterCardProps } from "./CounterCard";
import "./style.css";

function mountIslands() {
  document
    .querySelectorAll<HTMLElement>('[data-aegis-island="react-counter"]')
    .forEach((el) => {
      if (el.dataset.mounted === "1") return;
      el.dataset.mounted = "1";

      let props: CounterCardProps = {
        title: "Interactive Counter",
        initial: 0,
        step: 1,
      };
      if (el.dataset.props) {
        try {
          props = { ...props, ...JSON.parse(el.dataset.props) };
        } catch {
          // Keep default props if malformed
        }
      }
      createRoot(el).render(<CounterCard {...props} />);
    });
}

if (document.readyState === "loading") {
  document.addEventListener("DOMContentLoaded", mountIslands);
} else {
  mountIslands();
}
