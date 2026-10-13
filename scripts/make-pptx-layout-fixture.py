"""Generate independent placeholder geometry fixtures using python-pptx."""
from pathlib import Path
from copy import deepcopy
from pptx import Presentation
from pptx.util import Inches

folder = Path(__file__).resolve().parents[1] / "tests/fixtures"
deck = Presentation()
slide = deck.slides.add_slide(deck.slide_layouts[0])
slide.shapes.title.text = "Inherited title"
slide.placeholders[1].text = "Inherited subtitle"
box = slide.shapes.add_textbox(Inches(1), Inches(4), Inches(6), Inches(1))
box.text = "Direct position"
deck.save(folder / "layout-placeholders.pptx")
layout = deck.slide_layouts[0]
duplicate = deepcopy(layout.placeholders[0]._element)
layout.shapes._spTree.insert_element_before(duplicate, "p:extLst")
deck.save(folder / "layout-placeholders-ambiguous.pptx")
for shape in slide.shapes:
    print(shape.name, shape.left, shape.top, shape.width, shape.height)

# Restore unique layout index, remove title geometry so it inherits master.
duplicate.getparent().remove(duplicate)
for idx in [0, 1]:
    transform = layout.placeholders[idx]._element.spPr.xfrm
    transform.getparent().remove(transform)
deck.save(folder / "master-placeholders.pptx")
title = layout.placeholders[0]
print("Master title:", title.left, title.top, title.width, title.height)
body = layout.placeholders[1]
print("Master body:", body.left, body.top, body.width, body.height)
master = deck.slide_master
duplicate = deepcopy(master.placeholders[0]._element)
master.shapes._spTree.insert_element_before(duplicate, "p:extLst")
deck.save(folder / "master-placeholders-ambiguous.pptx")
