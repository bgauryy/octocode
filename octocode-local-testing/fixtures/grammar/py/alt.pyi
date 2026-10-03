LIMIT = 3

class Widget:
    def area(self):
        return helper(LIMIT)

def helper(n):
    return n * 2

def run():
    s = "😀"; return helper(len(s))
