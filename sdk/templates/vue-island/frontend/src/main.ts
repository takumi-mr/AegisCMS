import { createApp } from "vue";
import CounterCard from "./CounterCard.vue";
import "./style.css";

interface CounterCardProps {
  title: string;
  initial: number;
  step: number;
}

function mountIslands() {
  document
    .querySelectorAll<HTMLElement>('[data-aegis-island="vue-counter"]')
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
      createApp(CounterCard, props).mount(el);
    });
}

if (document.readyState === "loading") {
  document.addEventListener("DOMContentLoaded", mountIslands);
} else {
  mountIslands();
}
