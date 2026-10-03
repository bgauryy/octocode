"""graph.bin reader shared by the graph-bench scripts (mirrors store/format.rs, v2)."""
import struct

NODE_FILE, NODE_SYMBOL, NODE_PACKAGE = 0, 1, 2
EDGE_CONTAINS, EDGE_IMPORTS, EDGE_CALLS, EDGE_USES, EDGE_INHERITS = 0, 1, 2, 3, 4
NONE = 0xFFFFFFFF
KEY_SUFFIX = 0x80


class _Reader:
    def __init__(self, data):
        self.data, self.at = data, 0

    def u8(self):
        value = self.data[self.at]
        self.at += 1
        return value

    def var(self):
        value, shift = 0, 0
        while True:
            byte = self.u8()
            value |= (byte & 0x7F) << shift
            if byte < 0x80:
                return value
            shift += 7

    def opt(self):
        value = self.var()
        return NONE if value == 0 else value - 1

    def take(self, n):
        out = self.data[self.at:self.at + n]
        self.at += n
        return out

    def var_vec(self):
        return [self.var() for _ in range(self.var())]


def _unzigzag(value):
    return (value >> 1) ^ -(value & 1)


def _sections(path):
    b = open(path, "rb").read()
    assert b[:8] == b"OCGRAPH\0", "not a graph.bin"
    version, count = struct.unpack_from("<II", b, 8)
    assert version == 2, f"graph.bin format v{version}; this reader understands v2"
    sections = {}
    for i in range(count):
        at = 48 + i * 20
        tag = b[at:at + 4].decode()
        off, ln = struct.unpack_from("<QQ", b, at + 4)
        sections[tag] = b[off:off + ln]
    r = _Reader(sections["STRS"])
    strings, previous = [], b""
    for _ in range(r.var()):
        shared, suffix = r.var(), r.var()
        previous = previous[:shared] + r.take(suffix)
        strings.append(previous.decode())
    return sections, strings


def decode(path):
    sections, strings = _sections(path)
    r = _Reader(sections["NODE"])
    nodes = []
    for _ in range(r.var()):
        kind_byte, flags = r.u8(), r.u8()
        key, name, detail = r.var(), r.var(), r.var()
        file, parent, line = r.opt(), r.opt(), r.opt()
        if line == NONE:
            end = r.opt()
        else:
            delta = r.var()
            end = NONE if delta == 0 else line + _unzigzag(delta - 1)
        nodes.append(dict(kind=kind_byte & ~KEY_SUFFIX, flags=flags, key=strings[key],
                          suffix=bool(kind_byte & KEY_SUFFIX), name=strings[name],
                          detail=strings[detail], file=file, parent=parent, line=line, end_line=end))
    for node in nodes:
        if node.pop("suffix"):
            node["key"] = f'{nodes[node["file"]]["key"]}#{node["key"]}'
    r = _Reader(sections["EDGE"])
    edges, src = [], 0
    for _ in range(r.var()):
        src += r.var()
        dst = src + _unzigzag(r.var())
        packed = r.u8()
        detail, line = r.var(), r.opt()
        edges.append(dict(src=src, dst=dst, kind=packed >> 2, conf=packed & 3,
                          detail=strings[detail], line=line))
    return nodes, edges


def components(path):
    """FCMP section: file node id -> (component dir, component name, meta)."""
    sections, strings = _sections(path)
    if "FCMP" not in sections:
        return {}
    flat = _Reader(sections["FCMP"]).var_vec()
    return {flat[i]: (strings[flat[i + 1]], strings[flat[i + 2]], strings[flat[i + 3]])
            for i in range(0, len(flat), 4)}


def file_imports(nodes, edges):
    """Distinct (importer key, target key) pairs of linked file->file imports."""
    return {(nodes[e["src"]]["key"], nodes[e["dst"]]["key"]) for e in edges
            if e["kind"] == EDGE_IMPORTS and nodes[e["src"]]["kind"] == NODE_FILE
            and nodes[e["dst"]]["kind"] == NODE_FILE}
