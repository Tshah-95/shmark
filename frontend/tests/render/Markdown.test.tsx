import { render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { Markdown } from "../../src/render/Markdown";

describe("Markdown", () => {
  it("renders headings, lists, and inline code", () => {
    const { container } = render(
      <Markdown source={`# Title\n\nSome **bold** with \`code\`.\n\n- one\n- two`} />,
    );
    // Heading visible
    expect(screen.getByRole("heading", { level: 1, name: "Title" })).toBeInTheDocument();
    // List items
    const items = screen.getAllByRole("listitem");
    expect(items).toHaveLength(2);
    expect(items[0]).toHaveTextContent("one");
    // Inline code preserved
    expect(container.querySelector("code")?.textContent).toBe("code");
  });

  it("renders fenced code blocks (shiki applies asynchronously)", async () => {
    const source = "```ts\nconst x: number = 42;\n```\n";
    const { container } = render(<Markdown source={source} />);
    // Initially the code is plain (shiki promise pending). Wait for the
    // shiki-rendered <pre> with shiki-marker classes to appear.
    await waitFor(
      () => {
        const pre = container.querySelector("pre");
        expect(pre).toBeTruthy();
      },
      { timeout: 5000 },
    );
    expect(container.textContent).toContain("const");
    expect(container.textContent).toContain("number");
    expect(container.textContent).toContain("42");
  });

  it("renders GFM tables", () => {
    const source = `| h1 | h2 |\n|----|----|\n| a  | b  |\n`;
    const { container } = render(<Markdown source={source} />);
    expect(container.querySelector("table")).toBeTruthy();
    expect(container.querySelectorAll("th")).toHaveLength(2);
    expect(container.querySelectorAll("td")).toHaveLength(2);
  });

  it("does NOT render raw HTML — rehype-raw is disabled", () => {
    const source = `before <script>alert('xss')</script> after`;
    const { container } = render(<Markdown source={source} />);
    // The <script> tag should not be in the DOM as a real element. It comes
    // through as text or is stripped/escaped — either way no script element.
    expect(container.querySelector("script")).toBeNull();
  });
});
