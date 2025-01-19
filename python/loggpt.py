import math
import numpy as np
from tinygrad import Tensor, nn
from dataclasses import dataclass

@dataclass
class GPTConfig:
	block_size: int = 1024
	vocab_size: int = 20
	n_layer: int = 12
	n_head: int = 8
	embed_dim: int = 128

class CasualSelfAttention:
	def __init__(self, config: GPTConfig):
		self.attention = nn.Linear(config.embed_dim, config.embed_dim * 3)
		self.proj = nn.Linear(config.embed_dim, config.embed_dim)
		self.heads = config.n_head
		self.embed_dim = config.embed_dim
		self.bias = Tensor.ones(1, 1, config.block_size, config.block_size).tril()
		self.bias.requires_grad = False
	def __call__(self, x: Tensor) -> Tensor:
		B, T, C = x.shape
		qkv = self.attention(x)
		q, k, v = qkv.split(self.embed_dim, dim=2)
		k = k.view(B, T, self.heads, C // self.heads).transpose(1, 2)
		q = q.view(B, T, self.heads, C // self.heads).transpose(1, 2)
		v = v.view(B, T, self.heads, C // self.heads).transpose(1, 2)

		attention = (q @ k.transpose(-2, -1)) * (1.0 / math.sqrt(k.size(-1)))
		attention = attention.masked_fill(self.bias[:, :, :T, :T] == 0, float('-inf'))
		attention = attention.softmax()
		y = attention @ v
		y = y.transpose(1, 2).view(B, T, C)
		y = self.proj(y)
		return y

class MLP:
	def __init__(self, embed_dim: int):
		self.fc1 = nn.Linear(embed_dim, embed_dim * 4)
		self.fc2 = nn.Linear(embed_dim * 4, embed_dim)
	def __call__(self, x: Tensor) -> Tensor:
		return self.fc2(self.fc1(x).gelu())

class TransformerBlock:
	def __init__(self, config: GPTConfig):
		self.norm1 = nn.LayerNorm(config.embed_dim)
		self.attention = CasualSelfAttention(config)
		self.norm2 = nn.LayerNorm(config.embed_dim)
		self.mlp = MLP(config.embed_dim)

	def __call__(self, x: Tensor) -> Tensor:
		x = x + self.attention(self.norm1(x))
		x = x + self.mlp(self.norm2(x))
		return x

class LogGPT:
	def __init__(self, config: GPTConfig):
		self.config = config
		self.token_embedding = nn.Embedding(config.vocab_size, config.embed_dim)
		self.position_embedding = nn.Embedding(config.block_size, config.embed_dim)
		self.blocks = [TransformerBlock(config) for _ in range(config.n_layer)]
		self.norm = nn.LayerNorm(config.embed_dim)
		self.head = nn.Linear(config.embed_dim, config.vocab_size)

	def __call__(self, inx: Tensor, targets = None) -> Tensor:
		b, t = inx.shape
		pos = Tensor.arange(0, t)

		token_embedding = self.token_embedding(inx)
		pos_embedding = self.position_embedding(pos)
		x = token_embedding + pos_embedding
		x = self.norm(x.sequential(self.blocks))

		if targets is not None:
			logits = self.head(x)[:, -1, :self.config.vocab_size]
			loss = logits.sparse_categorical_crossentropy(targets)
		else:
			logits = self.head(x[:, [-1], :])[:, :, :self.config.vocab_size]
			loss = None

		return logits, loss
