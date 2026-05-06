import { render } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { CsvTable } from "../../src/render/CsvTable";

describe("CsvTable", () => {
  it("renders header + rows", () => {
    const { container } = render(
      <CsvTable source={`name,age,city\nAda,32,London\nGrace,28,NYC`} />,
    );
    const th = container.querySelectorAll("th");
    expect(th).toHaveLength(3);
    expect(th[0]?.textContent).toBe("name");
    expect(th[2]?.textContent).toBe("city");

    const td = container.querySelectorAll("td");
    expect(td).toHaveLength(6);
    expect(td[0]?.textContent).toBe("Ada");
    expect(td[5]?.textContent).toBe("NYC");
  });

  it("respects quoted fields with embedded commas", () => {
    const { container } = render(
      <CsvTable source={`name,note\nAda,"hello, world"\n`} />,
    );
    const td = container.querySelectorAll("td");
    expect(td).toHaveLength(2);
    expect(td[0]?.textContent).toBe("Ada");
    expect(td[1]?.textContent).toBe("hello, world");
  });

  it("handles empty input", () => {
    const { container } = render(<CsvTable source={""} />);
    expect(container.textContent).toContain("empty csv");
  });
});
