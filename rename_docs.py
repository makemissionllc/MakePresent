import pathlib

def process_readme():
    p = pathlib.Path("README.md")
    text = p.read_text(encoding="utf-8")
    # Title first mention
    text = text.replace("# MakePresent\n", "# MakrStudio (formerly MakePresent)\n", 1)
    # Bulk replace remaining MakePresent -> MakrStudio
    text = text.replace("MakePresent", "MakrStudio")
    # Revert protected parenthetical back to (formerly MakePresent)
    text = text.replace("(formerly MakrStudio)", "(formerly MakePresent)")
    # Revert file-path like artifacts that should stay as original repo/assets
    # Keep MakePresentIcons.zip original (file asset)
    text = text.replace("MakrStudioIcons.zip", "MakePresentIcons.zip")
    # Keep directory tree root as MakePresent/ if needed? But instruction says display name change,
    # so the tree illustration could stay as MakePresent/ to reflect actual repo path.
    # The earlier global replaced it to MakrStudio/ — revert to preserve file path instruction.
    # We revert only the tree line: "MakrStudio/" at start of code block.
    # The tree line is exactly "MakrStudio/" with backticks — we revert to MakePresent/
    # But after bulk replace, it became "MakrStudio/" — we want to keep as "MakePresent/" per Do NOT rename file paths.
    # So revert that specific occurrence.
    text = text.replace("```\nMakrStudio/\n", "```\nMakePresent/\n")
    # Also handle "src-tauri/src/project.rs:230 Project { show_text" etc not affected.
    p.write_text(text, encoding="utf-8")
    print("README done", text.count("MakrStudio"), text.count("MakePresent"))

def process_project():
    p = pathlib.Path("docs/PROJECT.md")
    text = p.read_text(encoding="utf-8")
    text = text.replace("# MakePresent", "# MakrStudio (formerly MakePresent)", 1)
    # For remaining prose, replace MakePresent -> MakrStudio
    text = text.replace("MakePresent", "MakrStudio")
    text = text.replace("(formerly MakrStudio)", "(formerly MakePresent)")
    # Keep any file path references? docs/PROJECT.md has no file path MakePresent/ tree, so no revert needed.
    p.write_text(text, encoding="utf-8")
    print("PROJECT done", text.count("MakrStudio"), text.count("MakePresent"))

process_readme()
process_project()
print("done")
