def parse_document(file_path: str) -> str:
    import json
    import os
    import sys
    import warnings
    
    # Ensure local virtualenv is in sys.path so PyO3 can find installed dependencies
    venv_win = os.path.abspath(os.path.join(os.getcwd(), ".venv", "Lib", "site-packages"))
    if os.path.exists(venv_win) and venv_win not in sys.path:
        sys.path.insert(0, venv_win)
        
    warnings.filterwarnings("ignore")
    import logging
    logging.disable(logging.WARNING)

    os.environ["HF_HUB_DISABLE_SYMLINKS_WARNING"] = "1"
    os.environ["HF_HUB_DISABLE_SYMLINKS"] = "1"
    os.environ.setdefault("TOKENIZERS_PARALLELISM", "false")

    if os.environ.get("ENABLE_DOCLING", "false").lower() not in ("true", "1"):
        raise RuntimeError("Docling disabled by configuration (ENABLE_DOCLING!=true) to prevent OOM")

    # Strategy 1: Docling
    try:
        # pyrefly: ignore [missing-import]
        from docling.document_converter import DocumentConverter
        converter = DocumentConverter()
        result = converter.convert(file_path)
        doc = result.document
        full_md = doc.export_to_markdown()
        sections = []
        tables = []
        current_heading = ""
        current_level = 1
        current_content_parts = []
        for item, _level in doc.iterate_items():
            item_type = type(item).__name__
            if item_type in ("SectionHeaderItem",):
                if current_content_parts:
                    content_text = "\n\n".join(current_content_parts).strip()
                    if content_text:
                        sections.append({"heading": current_heading, "level": current_level, "content": content_text})
                    current_content_parts = []
                current_heading = item.text if hasattr(item, "text") else str(item)
                current_level = getattr(item, "level", _level) or _level
                if isinstance(current_level, int): current_level = max(1, min(6, current_level))
                else: current_level = 1
            elif item_type in ("TextItem", "ListItem"):
                text = item.text if hasattr(item, "text") else str(item)
                if text.strip(): current_content_parts.append(text.strip())
            elif item_type in ("TableItem",):
                try: table_md = item.export_to_markdown()
                except Exception: table_md = str(item)
                caption = str(item.caption) if hasattr(item, "caption") and item.caption else ""
                tables.append({"caption": caption, "markdown": table_md})
            elif item_type == "CodeItem":
                code_text = item.text if hasattr(item, "text") else str(item)
                if code_text.strip():
                    lang = getattr(item, "language", "")
                    # Try to infer JSON if language is missing
                    if not lang and (code_text.strip().startswith("{") or code_text.strip().startswith("[")):
                        lang = "json"
                    current_content_parts.append(f"```{lang}\n{code_text}\n```")
            else:
                # Handle everything else (anomalies) thoroughly
                try:
                    anomaly_text = item.export_to_markdown()
                except Exception:
                    anomaly_text = getattr(item, "text", str(item))
                if anomaly_text and str(anomaly_text).strip():
                    current_content_parts.append(str(anomaly_text).strip())
        if current_content_parts:
            content_text = "\n\n".join(current_content_parts).strip()
            if content_text: sections.append({"heading": current_heading, "level": current_level, "content": content_text})
        if not sections and full_md.strip():
            paragraphs = [p.strip() for p in full_md.split("\n\n") if p.strip() and len(p.strip()) > 20]
            for i, para in enumerate(paragraphs): sections.append({"heading": f"Section {i + 1}" if len(paragraphs) > 1 else "", "level": 1, "content": para})
        title = sections[0]["heading"] if sections and sections[0]["heading"] else os.path.splitext(os.path.basename(file_path))[0]
        word_count = len(full_md.split())
        page_count = len(doc.pages) if hasattr(doc, "pages") else getattr(doc, "num_pages", 0)
        ext = os.path.splitext(file_path)[1].lstrip(".").lower()
        return json.dumps({"title": title, "sections": sections, "tables": tables, "metadata": {"page_count": page_count, "format": ext, "word_count": word_count, "parser": "advanced_pipeline"}}, ensure_ascii=False)
    except Exception as e1:
        pass

    # Strategy 2: Unstructured
    try:
        raise Exception("Skipping Unstructured for debugging")
        from unstructured.partition.auto import partition
        elements = partition(filename=file_path)
        sections, tables, current_heading, current_level, current_content_parts = [], [], "", 1, []
        for el in elements:
            el_type = type(el).__name__
            text = str(el).strip()
            if not text: continue
            if el_type.startswith("Title"):
                if current_content_parts:
                    sections.append({"heading": current_heading, "level": current_level, "content": "\n\n".join(current_content_parts)})
                    current_content_parts = []
                current_heading = text
                current_level = 1 if el_type == "Title" else 2
            elif el_type == "Table":
                tables.append({"caption": "", "markdown": text})
            else:
                current_content_parts.append(text)
        if current_content_parts: sections.append({"heading": current_heading, "level": current_level, "content": "\n\n".join(current_content_parts)})
        title = sections[0]["heading"] if sections and sections[0]["heading"] else os.path.splitext(os.path.basename(file_path))[0]
        full_text = "\n\n".join([s["content"] for s in sections])
        ext = os.path.splitext(file_path)[1].lstrip(".").lower()
        return json.dumps({"title": title, "sections": sections, "tables": tables, "metadata": {"page_count": 0, "format": ext, "word_count": len(full_text.split()), "parser": "unstructured"}}, ensure_ascii=False)
    except Exception as e2:
        pass

    # Strategy 3: Ultimate Fallback
    try:
        ext = os.path.splitext(file_path)[1].lower()
        text = ""
        if ext == ".pdf":
            import pypdfium2 as pdfium
            doc = pdfium.PdfDocument(file_path)
            pages = [page.get_textpage().get_text_range() for page in doc]
            text = "\n\n".join(pages)
        else:
            with open(file_path, "r", encoding="utf-8", errors="replace") as f: text = f.read()
        paragraphs = [p.strip() for p in text.split("\n\n") if p.strip() and len(p.strip()) > 20]
        if not paragraphs and text.strip(): paragraphs = [text.strip()]
        sections = [{"heading": f"Section {i + 1}" if len(paragraphs) > 1 else "", "level": 1, "content": para} for i, para in enumerate(paragraphs)]
        title = os.path.splitext(os.path.basename(file_path))[0]
        return json.dumps({"title": title, "sections": sections, "tables": [], "metadata": {"page_count": 0, "format": ext.lstrip("."), "word_count": len(text.split()), "parser": "pypdfium2_or_text"}}, ensure_ascii=False)
    except Exception as e3:
        raise RuntimeError(f"All parsers failed. Fallback error: {e3}")
