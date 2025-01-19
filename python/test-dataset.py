
import pandas as pd

df = pd.read_csv("../dataset.csv")

print(df.head())

data = df[:50]
print(data)