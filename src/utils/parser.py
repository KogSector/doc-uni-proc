import json
import os
import sys
import warnings

# Global cached converter instance for lazy reuse
_CACHED_CONVERTER = None

def get_docling_converter():
    global _CACHED_CONVERTER
    if _CACHED_CONVERTER is None:
        # pyrefly: ignore [missing-import]
        from docling.document_converter import DocumentConverter
        _CACHED_CONVERTER = DocumentConverter()
    return _CACHED_CONVERTER

def parse_document(file_path: str) -> str:
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

    enable_docling = os.environ.get("ENABLE_DOCLING", "true").lower() in ("true", "1")

    # Strategy 1: Docling Slim (when enabled)
    if enable_docling:
        try:
            converter = get_docling_converter()
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
                    if isinstance(current_level, int):
                        current_level = max(1, min(6, current_level))
                    else:
                        current_level = 1
                elif item_type in ("TextItem", "ListItem"):
                    text = item.text if hasattr(item, "text") else str(item)
                    if text.strip():
                        current_content_parts.append(text.strip())
                elif item_type in ("TableItem",):
                    try:
                        table_md = item.export_to_markdown()
                    except Exception:
                        table_md = str(item)
                    caption = str(item.caption) if hasattr(item, "caption") and item.caption else ""
                    tables.append({"caption": caption, "markdown": table_md})
                elif item_type == "CodeItem":
                    code_text = item.text if hasattr(item, "text") else str(item)
                    if code_text.strip():
                        lang = getattr(item, "language", "")
                        if not lang and (code_text.strip().startswith("{") or code_text.strip().startswith("[")):
                            lang = "json"
                        current_content_parts.append(f"```{lang}\n{code_text}\n```")
                else:
                    try:
                        anomaly_text = item.export_to_markdown()
                    except Exception:
                        anomaly_text = getattr(item, "text", str(item))
                    if anomaly_text and str(anomaly_text).strip():
                        current_content_parts.append(str(anomaly_text).strip())
                        
            if current_content_parts:
                content_text = "\n\n".join(current_content_parts).strip()
                if content_text:
                    sections.append({"heading": current_heading, "level": current_level, "content": content_text})
                    
            if not sections and full_md.strip():
                paragraphs = [p.strip() for p in full_md.split("\n\n") if p.strip() and len(p.strip()) > 20]
                for i, para in enumerate(paragraphs):
                    sections.append({"heading": f"Section {i + 1}" if len(paragraphs) > 1 else "", "level": 1, "content": para})
                    
            title = sections[0]["heading"] if sections and sections[0]["heading"] else os.path.splitext(os.path.basename(file_path))[0]
            word_count = len(full_md.split())
            page_count = len(doc.pages) if hasattr(doc, "pages") else getattr(doc, "num_pages", 0)
            ext = os.path.splitext(file_path)[1].lstrip(".").lower()
            return json.dumps({
                "title": title,
                "sections": sections,
                "tables": tables,
                "metadata": {"page_count": page_count, "format": ext, "word_count": word_count, "parser": "docling-slim"}
            }, ensure_ascii=False)
        except Exception:
            pass

    # Strategy 2: Fast PDFium or Text reader (Fallback)
    try:
        ext = os.path.splitext(file_path)[1].lower()
        text = ""
        page_count = 0
        if ext == ".pdf":
            try:
                import pypdfium2 as pdfium
                doc = pdfium.PdfDocument(file_path)
                page_count = len(doc)
                pages = [page.get_textpage().get_text_range() for page in doc]
                text = "\n\n".join(pages)
            except Exception:
                with open(file_path, "r", encoding="utf-8", errors="replace") as f:
                    text = f.read()
        else:
            with open(file_path, "r", encoding="utf-8", errors="replace") as f:
                text = f.read()
                
        paragraphs = [p.strip() for p in text.split("\n\n") if p.strip() and len(p.strip()) > 20]
        if not paragraphs and text.strip():
            paragraphs = [text.strip()]
        sections = [{"heading": f"Section {i + 1}" if len(paragraphs) > 1 else "", "level": 1, "content": para} for i, para in enumerate(paragraphs)]
        title = os.path.splitext(os.path.basename(file_path))[0]
        return json.dumps({
            "title": title,
            "sections": sections,
            "tables": [],
            "metadata": {"page_count": page_count, "format": ext.lstrip("."), "word_count": len(text.split()), "parser": "pypdfium2_or_text"}
        }, ensure_ascii=False)
    except Exception as e3:
        raise RuntimeError(f"All python document parsers failed: {e3}")
