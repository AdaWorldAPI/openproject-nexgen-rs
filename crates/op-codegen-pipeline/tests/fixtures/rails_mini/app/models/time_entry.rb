class TimeEntry < ApplicationRecord
  belongs_to :work_package

  validates :hours, presence: true
end
