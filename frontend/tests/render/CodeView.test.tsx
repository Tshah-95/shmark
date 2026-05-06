import { render, waitFor } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { CodeView } from "../../src/render/CodeView";

describe("CodeView", () => {
  it("renders typescript content with shiki", async () => {
    const { container } = render(
      <CodeView source={`const x: number = 42;`} lang="typescript" />,
    );
    await waitFor(
      () => {
        expect(container.querySelector("pre")).toBeTruthy();
      },
      { timeout: 5000 },
    );
    expect(container.textContent).toContain("number");
    expect(container.textContent).toContain("42");
  });

  it("falls back to plain pre/code on unsupported language", async () => {
    const { container } = render(
      <CodeView source={`hello world`} lang="this-is-not-a-real-lang" />,
    );
    // Wait a tick so the failed shiki promise resolves
    await new Promise((r) => setTimeout(r, 50));
    const pre = container.querySelector("pre");
    expect(pre).toBeTruthy();
    expect(pre?.textContent).toContain("hello world");
  });
});
